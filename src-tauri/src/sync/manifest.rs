//! 版本化备份清单。
//!
//! ## 为什么需要它
//!
//! 原实现的备份是「一个 `.db.gz` 文件」，没有任何自描述信息。恢复时无法回答
//! 「这个备份是什么时候生成的」「里面有多少作品」「图片是否齐全」，
//! 只能盲目覆盖。作者看到「恢复成功」时，其实无从判断恢复出来的数据是否完整。
//!
//! 清单是**旁挂文件**（`backup_xxx.db.gz.json`），不是数据库内部的新表。
//! 这样做的关键理由是**向后兼容**：没有清单的历史备份依然可以被恢复
//! （走 [`crate::sync::backup::preflight_backup`] 的结构校验分支），
//! 而不是因为「缺清单」被一刀切拒绝。
//!
//! ## 安全约束
//!
//! 清单会随备份上传到用户的 WebDAV，所以**绝不能包含任何凭据**：
//! API Key、WebDAV 密码、钥匙串 key_ref、服务器地址一律不写入。
//! 这里只记录实体计数与图片的 `rel_path` / `size` / `sha256` ——
//! 这些都是恢复完整性判断真正需要、且不含秘密的信息。
//!
//! 清单同时是**不可信输入**：它来自远端，可能被篡改、被截断、或来自另一份备份。
//! 因此 [`verify_manifest`] 对表名走白名单（绝不把清单里的字符串拼进 SQL），
//! 对资产路径做穿越校验（绝不直接 join 远端给出的路径），
//! 并把「清单损坏」「清单版本不支持」「清单与备份不匹配」与「清单不存在」
//! 严格区分开 —— 后者是历史备份的正常兼容路径，前三者必须拒绝覆盖。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// 清单格式版本。**只有当字段语义发生不兼容变化时才递增**，
/// 读取方必须能处理比自己旧的版本。
///
/// v2 相对 v1 增加了 `compressed_sha256` 与 `missing_assets` 两个字段，
/// 都是**附加**字段（`#[serde(default)]`），v1 清单仍可解析，
/// 因此 `MIN_SUPPORTED_MANIFEST_VERSION` 保持为 1。
pub const MANIFEST_VERSION: u32 = 2;

/// 当前客户端仍能读懂的最旧清单版本。低于它的清单必须被明确拒绝，
/// 不能因为「解析不出来」就退化成历史无清单备份。
pub const MIN_SUPPORTED_MANIFEST_VERSION: u32 = 1;

/// 需要在清单中留痕的实体表。刻意不包含 `llm_profile` / `webdav_config`
/// —— 前者涉及钥匙串引用，后者存服务器地址，都属于不该出现在备份里的信息。
///
/// 这个列表同时是 [`verify_manifest`] 的**表名白名单**：
/// 清单里的 `entity_counts` 键必须落在其中，否则拒绝校验。
/// 少了这层白名单，一份被篡改的清单就能让恢复流程执行
/// `SELECT COUNT(*) FROM <任意字符串>`。
const COUNTED_TABLES: &[&str] = &[
    "novel",
    "volume",
    "chapter",
    "chapter_revision",
    "worldview_entry",
    "character",
    "character_relation",
    "plot_item",
    "extraction_candidate",
    "setting_change",
    "novel_lexicon",
];

/// 资产相对路径必须落在此目录内。
const ASSET_DIR_PREFIX: &str = "images/";

/// 单个图片资产的完整性描述。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetEntry {
    /// 形如 `images/<sha256>.png` 的库内相对路径。
    pub rel_path: String,
    /// 字节数。
    pub size: i64,
    /// 内容 SHA-256 十六进制摘要（与 `asset.rs` 保存时算法一致）。
    pub hash: String,
}

/// 一次备份的自描述清单。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    /// 清单格式版本，用于前向兼容判断。
    pub manifest_version: u32,
    /// 生成时间（RFC3339，UTC）。
    pub created_at: String,
    /// 生成该备份的应用版本。
    pub app_version: String,
    /// 备份所含数据库的 schema 版本。
    pub schema_version: i64,
    /// 各实体表行数。
    pub entity_counts: BTreeMap<String, i64>,
    /// 备份涉及的图片资产清单。
    pub assets: Vec<AssetEntry>,
    /// 压缩后 `.db.gz` 的字节数。
    pub compressed_size_bytes: u64,
    /// 压缩包的 SHA-256。把清单**绑定**到具体某一份备份包上：
    /// 少了它，把 A 备份的清单挂到 B 备份旁边不会被发现（计数恰好相同时）。
    ///
    /// v1 清单没有这个字段，反序列化为空串，此时该项校验降级为提示。
    #[serde(default)]
    pub compressed_sha256: String,
    /// 数据库 `asset` 表有记录、但**备份时本地文件已缺失**的资产 `rel_path`。
    ///
    /// 存在的理由：[`reconcile_with_disk`] 会把缺文件的资产从 `assets` 里剔除
    /// （清单只应描述真实存在于备份中的东西），但剔除动作本身会让
    /// 「备份时就已经丢图」这件事消失。这个字段把它永久留在清单里，
    /// 恢复时才能明确告诉作者哪些素材从一开始就没进备份。
    #[serde(default)]
    pub missing_assets: Vec<String>,
}

/// 从一个已打开的数据库连接构建清单。
///
/// ## 为什么返回 `Result`
///
/// 原实现对 schema 版本和每张表的计数都用 `unwrap_or(0)`，
/// 对资产查询失败直接返回空列表。后果是：一次查询失败会生成一份
/// **看起来完全正常、计数全为 0、资产为空**的清单并上传到远端。
/// 恢复时拿它做交叉校验，结论必然是「备份里什么都没有，与清单一致」——
/// 校验通过了，但校验的是伪造数据。
///
/// 现在任何一项统计读不出来都让备份清单生成失败，由调用方决定如何呈现。
pub fn build_manifest(
    conn: &Connection,
    app_version: &str,
    compressed_size_bytes: u64,
    compressed_sha256: &str,
) -> Result<BackupManifest, String> {
    let schema_version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| format!("读取 schema 版本失败，拒绝生成计数不可信的备份清单：{e}"))?;

    let mut entity_counts = BTreeMap::new();
    for table in COUNTED_TABLES {
        // 表名来自本文件的常量白名单，不含任何外部输入。
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count = conn
            .query_row(&sql, [], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("统计 {table} 失败，拒绝生成计数不可信的备份清单：{e}"))?;
        entity_counts.insert((*table).to_string(), count);
    }

    let assets = collect_assets(conn)?;

    Ok(BackupManifest {
        manifest_version: MANIFEST_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        app_version: app_version.to_string(),
        schema_version,
        entity_counts,
        assets,
        compressed_size_bytes,
        compressed_sha256: compressed_sha256.to_string(),
        missing_assets: Vec::new(),
    })
}

/// 读取资产表。查询失败必须冒泡 —— 返回空列表等于宣称「这个库没有图片」。
fn collect_assets(conn: &Connection) -> Result<Vec<AssetEntry>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path, size, hash FROM asset ORDER BY rel_path")
        .map_err(|e| format!("读取资产表失败，无法确认备份中的图片完整性：{e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AssetEntry {
                rel_path: row.get(0)?,
                size: row.get(1)?,
                hash: row.get(2)?,
            })
        })
        .map_err(|e| format!("查询资产表失败：{e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取资产记录失败：{e}"))
}

/// 用磁盘上的真实文件覆盖清单中资产的 `size`，并把缺文件的资产
/// 从 `assets` 移入 `missing_assets`。
///
/// 备份生成时数据库里的大小只反映「当初插入时的值」，
/// 而恢复要判断的是「文件是否真的在」。因此以磁盘为准。
///
/// 返回缺失的 `rel_path` 列表，调用方**必须**把它呈现给用户：
/// 缺图不影响数据库备份本身可用，但作者有权在备份当下就知道素材没进备份。
pub fn reconcile_with_disk(manifest: &mut BackupManifest, app_data_dir: &Path) -> Vec<String> {
    let mut missing = Vec::new();
    let mut kept: Vec<AssetEntry> = Vec::with_capacity(manifest.assets.len());
    for mut entry in std::mem::take(&mut manifest.assets) {
        match resolve_asset_path(app_data_dir, &entry.rel_path) {
            // 路径非法的资产不进清单：它无法被安全恢复，
            // 但也不能就此消失，记入 missing 让作者看见。
            Err(_) => missing.push(entry.rel_path),
            Ok(path) => match fs::metadata(&path) {
                Ok(meta) => {
                    entry.size = meta.len() as i64;
                    kept.push(entry);
                }
                Err(_) => missing.push(entry.rel_path),
            },
        }
    }
    manifest.assets = kept;
    manifest.missing_assets = missing.clone();
    missing
}

/// 校验资产相对路径是否可以安全地解析到应用数据目录内。
///
/// 清单来自远端，属于不可信输入。这里挡住的是：
/// 绝对路径、`../` 穿越、Windows 盘符与反斜杠、控制字符、
/// 以及任何落在 `images/` 之外的位置。
pub fn validate_asset_rel_path(rel: &str) -> Result<(), String> {
    if rel.trim().is_empty() {
        return Err("资产路径为空".to_string());
    }
    if rel.contains('\0') || rel.chars().any(|c| (c as u32) < 0x20) {
        return Err(format!("资产路径包含控制字符：{rel:?}"));
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err(format!("资产路径不得为绝对路径：{rel}"));
    }
    if rel.contains('\\') {
        return Err(format!("资产路径不得包含反斜杠：{rel}"));
    }
    if rel.contains(':') {
        return Err(format!("资产路径不得包含盘符：{rel}"));
    }
    for seg in rel.split('/') {
        if seg.is_empty() {
            return Err(format!("资产路径包含空路径段：{rel}"));
        }
        if seg == "." || seg == ".." {
            return Err(format!("资产路径包含相对路径段：{rel}"));
        }
    }
    if !rel.starts_with(ASSET_DIR_PREFIX) {
        return Err(format!(
            "资产路径必须位于 {ASSET_DIR_PREFIX} 目录内：{rel}"
        ));
    }
    Ok(())
}

/// 把已校验的相对路径解析成真实路径，并再确认一次它没有逃出应用数据目录。
pub fn resolve_asset_path(app_data_dir: &Path, rel: &str) -> Result<std::path::PathBuf, String> {
    validate_asset_rel_path(rel)?;
    let joined = app_data_dir.join(rel);
    if !joined.starts_with(app_data_dir) {
        return Err(format!("资产路径越出应用数据目录：{rel}"));
    }
    Ok(joined)
}

/// 清单校验结论。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestVerification {
    /// 必须拒绝恢复的原因。非空即表示清单不可信。
    pub fatal: Vec<String>,
    /// 只需告知作者、不阻断恢复的观察项。
    pub notes: Vec<String>,
}

impl ManifestVerification {
    pub fn is_ok(&self) -> bool {
        self.fatal.is_empty()
    }
}

/// 把清单与**已解压的候选数据库**逐项对照。
///
/// `actual_compressed_size` / `actual_compressed_sha256` 是本次真实下载到的
/// 压缩包度量值；传 `None` 表示调用方没有该信息（此时跳过对应校验，
/// 而不是假装通过）。
///
/// 返回 `Err` 只表示「校验本身无法进行」（例如数据库打不开）；
/// 清单与备份不一致属于 `Ok` + `fatal` 非空，这样调用方能拿到全部不一致项
/// 一次性呈现，而不是修一个报一个。
pub fn verify_manifest(
    manifest: &BackupManifest,
    db_path: &Path,
    actual_compressed_size: Option<u64>,
    actual_compressed_sha256: Option<&str>,
    app_data_dir: Option<&Path>,
) -> Result<ManifestVerification, String> {
    let mut v = ManifestVerification::default();

    // ---- 1. 清单格式版本 ----
    if manifest.manifest_version < MIN_SUPPORTED_MANIFEST_VERSION
        || manifest.manifest_version > MANIFEST_VERSION
    {
        v.fatal.push(format!(
            "备份清单格式版本 v{} 不受当前客户端支持（支持 v{}–v{}）。请升级软件后再恢复这份备份。",
            manifest.manifest_version, MIN_SUPPORTED_MANIFEST_VERSION, MANIFEST_VERSION
        ));
        // 版本都不认识，后面的字段语义无法保证，直接返回。
        return Ok(v);
    }

    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("无法打开候选备份以校验清单：{e}"))?;

    // ---- 2. schema 版本 ----
    let actual_schema: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("读取候选备份 schema 版本失败：{e}"))?;
    if manifest.schema_version != actual_schema {
        v.fatal.push(format!(
            "清单声明 schema v{}，但备份数据库实际是 v{}。",
            manifest.schema_version, actual_schema
        ));
    }
    if manifest.schema_version < 0 {
        v.fatal.push(format!(
            "清单声明了非法的 schema 版本：{}",
            manifest.schema_version
        ));
    }

    // ---- 3. 实体计数（全部表，不只作品与章节）----
    for table in COUNTED_TABLES {
        let declared = match manifest.entity_counts.get(*table) {
            Some(n) => *n,
            None => {
                v.fatal.push(format!("清单缺少 {table} 的实体计数，无法确认备份完整性。"));
                continue;
            }
        };
        if declared < 0 {
            v.fatal.push(format!("清单中 {table} 的计数为负数（{declared}）。"));
            continue;
        }
        let actual: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .map_err(|e| format!("统计候选备份的 {table} 失败：{e}"))?;
        if actual != declared {
            v.fatal.push(format!(
                "清单记录 {table} 有 {declared} 条，备份数据库实际有 {actual} 条。"
            ));
        }
    }
    // 清单里出现白名单之外的键：要么是更新版本写的，要么是被篡改过。
    // 两种情况都不能默默忽略。
    for key in manifest.entity_counts.keys() {
        if !COUNTED_TABLES.contains(&key.as_str()) {
            v.fatal.push(format!(
                "清单包含当前客户端不认识的实体表 {key:?}，拒绝按不可信的表名继续校验。"
            ));
        }
    }

    // ---- 4. 压缩包度量值 ----
    if let Some(actual) = actual_compressed_size {
        if manifest.compressed_size_bytes != actual {
            v.fatal.push(format!(
                "清单声明压缩包 {} 字节，实际下载 {} 字节（备份可能被截断或替换）。",
                manifest.compressed_size_bytes, actual
            ));
        }
    }
    if manifest.compressed_sha256.is_empty() {
        v.notes.push(
            "该清单为 v1 格式，未记录压缩包哈希，无法确认清单与这份备份包严格绑定。".to_string(),
        );
    } else if let Some(actual) = actual_compressed_sha256 {
        if !manifest.compressed_sha256.eq_ignore_ascii_case(actual) {
            v.fatal.push(
                "清单记录的压缩包哈希与实际下载的备份包不符：这份清单不属于该备份。".to_string(),
            );
        }
    }

    // ---- 5. 资产条目自身合法性 ----
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for entry in &manifest.assets {
        if let Err(e) = validate_asset_rel_path(&entry.rel_path) {
            v.fatal.push(format!("清单资产路径非法：{e}"));
            continue;
        }
        if !seen.insert(&entry.rel_path) {
            v.fatal.push(format!("清单中资产路径重复出现：{}", entry.rel_path));
        }
        if !is_sha256_hex(&entry.hash) {
            v.fatal.push(format!(
                "清单中 {} 的哈希不是合法的 SHA-256 十六进制值。",
                entry.rel_path
            ));
        }
        if entry.size < 0 {
            v.fatal.push(format!(
                "清单中 {} 声明了负数字节数（{}）。",
                entry.rel_path, entry.size
            ));
        }
    }

    // ---- 6. 清单资产 vs 数据库资产记录 ----
    let db_assets = collect_assets(&conn).map_err(|e| format!("校验清单时{e}"))?;
    let manifest_paths: BTreeSet<&str> = manifest.assets.iter().map(|a| a.rel_path.as_str()).collect();
    let db_paths: BTreeSet<&str> = db_assets.iter().map(|a| a.rel_path.as_str()).collect();

    for p in manifest_paths.difference(&db_paths) {
        v.fatal.push(format!(
            "清单声明了资产 {p}，但备份数据库的 asset 表中没有这条记录：清单与备份不匹配。"
        ));
    }
    for p in db_paths.difference(&manifest_paths) {
        // 数据库有记录、清单里没有 —— 正常原因是备份时文件已缺失
        // （见 missing_assets）。但如果它也不在 missing_assets 里，
        // 说明清单是在别的时刻生成的，不能当作已知情况放过。
        if manifest.missing_assets.iter().any(|m| m == *p) {
            v.notes.push(format!(
                "资产 {p} 在生成备份时文件已缺失，未进入备份包；恢复后该素材仍会缺失。"
            ));
        } else {
            v.fatal.push(format!(
                "备份数据库引用了资产 {p}，但清单中既没有它、也没有把它记为缺失：清单与备份不匹配。"
            ));
        }
    }
    if !manifest.missing_assets.is_empty() {
        v.notes.push(format!(
            "该备份生成时有 {} 个素材文件在本地已缺失，未包含在备份中。",
            manifest.missing_assets.len()
        ));
    }

    // ---- 7. 本机已有文件的内容核对（可选，不联网）----
    if let Some(dir) = app_data_dir {
        for entry in &manifest.assets {
            let Ok(path) = resolve_asset_path(dir, &entry.rel_path) else {
                continue; // 已在第 5 步记为 fatal
            };
            match fs::read(&path) {
                Err(_) => {} // 本地缺失属于正常情况，由恢复阶段回填
                Ok(bytes) => {
                    if bytes.len() as i64 != entry.size {
                        v.notes.push(format!(
                            "本机 {} 大小（{}）与清单（{}）不符，恢复时会用备份内容覆盖。",
                            entry.rel_path,
                            bytes.len(),
                            entry.size
                        ));
                    } else if sha256_hex(&bytes) != entry.hash {
                        v.notes.push(format!(
                            "本机 {} 内容与清单哈希不符，恢复时会用备份内容覆盖。",
                            entry.rel_path
                        ));
                    }
                }
            }
        }
    }

    Ok(v)
}

/// 是否为合法的 SHA-256 十六进制摘要。
///
/// 清单里的哈希来自远端，格式不合法就说明清单被篡改或已损坏，
/// 不能拿它去做内容比对（比对结果必然是「不匹配」，掩盖真实原因）。
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// 序列化清单到 sidecar 文件路径（`<backup>.json`）。
pub fn write_sidecar(backup_path: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let sidecar = sidecar_path_for(backup_path);
    if let Some(parent) = sidecar.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建清单目录失败: {e}"))?;
    }
    let text = serde_json::to_vec_pretty(manifest).map_err(|e| format!("序列化清单失败: {e}"))?;
    crate::sync::backup::atomic_write(&sidecar, &text)
}

/// 读取清单 sidecar。文件不存在返回 `Ok(None)`（= 历史备份，无清单）。
///
/// **文件存在但解析失败必须返回 `Err`**：把损坏的清单当成「没有清单」，
/// 等于让篡改或截断过的清单把恢复流程降级到只校验结构的兼容分支。
pub fn read_sidecar(backup_path: &Path) -> Result<Option<BackupManifest>, String> {
    let sidecar = sidecar_path_for(backup_path);
    match fs::read(&sidecar) {
        Ok(bytes) => {
            let m: BackupManifest = serde_json::from_slice(&bytes)
                .map_err(|e| format!("备份清单解析失败（{}）: {e}", sidecar.display()))?;
            Ok(Some(m))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取备份清单失败（{}）: {e}", sidecar.display())),
    }
}

pub fn sidecar_path_for(backup_path: &Path) -> std::path::PathBuf {
    let mut s = backup_path.as_os_str().to_os_string();
    s.push(".json");
    std::path::PathBuf::from(s)
}

/// 计算备份正文的 SHA-256（用于校验「下载到的字节 == 上传时的字节」）。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> BackupManifest {
        BackupManifest {
            manifest_version: MANIFEST_VERSION,
            created_at: "2026-01-01T00:00:00Z".into(),
            app_version: "0.3.8".into(),
            schema_version: 3,
            entity_counts: BTreeMap::from([("novel".to_string(), 1)]),
            assets: vec![AssetEntry {
                rel_path: "images/abc.png".into(),
                size: 1,
                hash: "0".repeat(64),
            }],
            compressed_size_bytes: 10,
            compressed_sha256: "a".repeat(64),
            missing_assets: Vec::new(),
        }
    }

    #[test]
    fn sidecar_path_appends_json() {
        let p = sidecar_path_for(Path::new("/tmp/backup_x.db.gz"));
        assert_eq!(p, Path::new("/tmp/backup_x.db.gz.json"));
    }

    #[test]
    fn sha256_matches_asset_module_algorithm() {
        // 已知向量：空输入的 SHA-256
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn manifest_json_contains_no_secret_shaped_fields() {
        // 结构性护栏：清单的字段名集合是封闭的，
        // 以后有人往里加字段时，这个测试会提醒他复核是否引入了秘密。
        let json = serde_json::to_string(&sample_manifest()).unwrap();
        for forbidden in ["key", "pwd", "password", "secret", "token", "url", "username"] {
            assert!(
                !json.to_lowercase().contains(forbidden),
                "清单 JSON 中不应出现敏感字段名片段：{forbidden}\n{json}"
            );
        }
    }

    #[test]
    fn asset_path_validation_rejects_escape_and_absolute() {
        for evil in [
            "../secret.png",
            "images/../../etc/passwd",
            "/etc/passwd",
            "C:\\images\\a.png",
            "images\\a.png",
            "images//a.png",
            "images/./a.png",
            "attachments/a.png",
            "",
            "   ",
            "images/a\u{0}.png",
        ] {
            assert!(
                validate_asset_rel_path(evil).is_err(),
                "非法资产路径必须被拒绝：{evil:?}"
            );
        }
        assert!(validate_asset_rel_path("images/abc.png").is_ok());
    }

    #[test]
    fn v1_manifest_without_new_fields_still_parses() {
        // 向后兼容护栏：老清单没有 compressed_sha256 / missing_assets，
        // 必须仍能解析，否则所有历史备份的清单都会被判为「损坏」。
        let v1 = r#"{
            "manifest_version": 1,
            "created_at": "2026-01-01T00:00:00Z",
            "app_version": "0.3.7",
            "schema_version": 3,
            "entity_counts": {"novel": 1},
            "assets": [],
            "compressed_size_bytes": 10
        }"#;
        let m: BackupManifest = serde_json::from_str(v1).unwrap();
        assert_eq!(m.manifest_version, 1);
        assert_eq!(m.compressed_sha256, "");
        assert!(m.missing_assets.is_empty());
        assert!(
            m.manifest_version >= MIN_SUPPORTED_MANIFEST_VERSION,
            "v1 必须仍在支持范围内"
        );
    }

    #[test]
    fn unsupported_manifest_version_is_detected() {
        // 版本范围检查必须真的能拦住未来版本与远古版本，
        // 否则「清单版本不支持」这条验收标准就是空话。
        for bad in [0, MANIFEST_VERSION + 1, 99] {
            let m = BackupManifest {
                manifest_version: bad,
                ..sample_manifest()
            };
            let dir = std::env::temp_dir().join(format!("ew_mfv_{}_{}", bad, std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let db = dir.join("x.db");
            std::fs::write(&db, b"not sqlite").unwrap();
            let v = verify_manifest(&m, &db, None, None, None).unwrap();
            assert!(
                v.fatal.iter().any(|f| f.contains("清单格式版本")),
                "版本 {bad} 必须被判定为不支持，实际：{v:?}"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

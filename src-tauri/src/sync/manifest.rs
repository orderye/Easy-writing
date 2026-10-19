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

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// 清单格式版本。**只有当字段语义发生不兼容变化时才递增**，
/// 读取方必须能处理比自己旧的版本。
pub const MANIFEST_VERSION: u32 = 1;

/// 需要在清单中留痕的实体表。刻意不包含 `llm_profile` / `webdav_config`
/// —— 前者涉及钥匙串引用，后者存服务器地址，都属于不该出现在备份里的信息。
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
}

/// 从一个已打开的数据库连接构建清单。
///
/// `app_data_dir` 用于把 `asset.rel_path` 解析成真实文件以读取大小与哈希；
/// 文件缺失时该项会被跳过（缺失情况由 `scan_storage` / 恢复报告负责暴露，
/// 清单不应该因为一张缺图就整个生成失败）。
pub fn build_manifest(
    conn: &Connection,
    app_version: &str,
    compressed_size_bytes: u64,
) -> BackupManifest {
    let schema_version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);

    let mut entity_counts = BTreeMap::new();
    for table in COUNTED_TABLES {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count = conn
            .query_row(&sql, [], |row| row.get::<_, i64>(0))
            .unwrap_or(0);
        entity_counts.insert((*table).to_string(), count);
    }

    let assets = collect_assets(conn);

    BackupManifest {
        manifest_version: MANIFEST_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        app_version: app_version.to_string(),
        schema_version,
        entity_counts,
        assets,
        compressed_size_bytes,
    }
}

fn collect_assets(conn: &Connection) -> Vec<AssetEntry> {
    let mut stmt = match conn.prepare("SELECT rel_path, size, hash FROM asset ORDER BY rel_path") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |row| {
        Ok(AssetEntry {
            rel_path: row.get(0)?,
            size: row.get(1)?,
            hash: row.get(2)?,
        })
    });
    match rows {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// 用磁盘上的真实文件覆盖清单中资产的 `size`，并在缺文件时剔除该项。
///
/// 备份生成时数据库里的大小只反映「当初插入时的值」，
/// 而恢复要判断的是「文件是否真的在」。因此以磁盘为准。
pub fn reconcile_with_disk(
    manifest: &mut BackupManifest,
    app_data_dir: &Path,
) -> Vec<String> {
    let mut missing = Vec::new();
    let mut kept: Vec<AssetEntry> = Vec::with_capacity(manifest.assets.len());
    for mut entry in std::mem::take(&mut manifest.assets) {
        match fs::metadata(app_data_dir.join(&entry.rel_path)) {
            Ok(meta) => {
                entry.size = meta.len() as i64;
                kept.push(entry);
            }
            Err(_) => missing.push(entry.rel_path),
        }
    }
    manifest.assets = kept;
    missing
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

fn sidecar_path_for(backup_path: &Path) -> std::path::PathBuf {
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
        let m = BackupManifest {
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
        };
        let json = serde_json::to_string(&m).unwrap();
        for forbidden in ["key", "pwd", "password", "secret", "token", "url", "username"] {
            assert!(
                !json.to_lowercase().contains(forbidden),
                "清单 JSON 中不应出现敏感字段名片段：{forbidden}\n{json}"
            );
        }
    }
}

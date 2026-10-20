use crate::db::DbPool;
use crate::sync::manifest;
use crate::sync::webdav::WebDavClient;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 恢复流程可接受的最大解压体积（256 MiB）。
///
/// 备份来自用户自己的 WebDAV，但这仍是一个「解压到内存」的动作，
/// 没有上限就等于让远端异常（或被他人篡改）决定本机内存用量。
pub const MAX_RESTORE_DECOMPRESSED_BYTES: usize = 256 * 1024 * 1024;

/// 备份文件名白名单校验。
///
/// 前端传入的 `filename` 会直接拼进 WebDAV URL，而 `WebDavClient::build_url`
/// 只做字符串拼接、不做规范化。因此 `../../other/secret.db.gz` 这类输入
/// 会让恢复流程离开 `remote_dir`，读取账号可读范围内的任意 `.db.gz`。
///
/// 这里在**最靠近入口**的地方把它挡住：只允许「单个文件名」，
/// 不允许任何路径分隔符、相对段、或 Windows 盘符/反斜杠。
pub fn validate_backup_filename(filename: &str) -> Result<(), String> {
    if filename.trim().is_empty() {
        return Err("备份文件名不能为空".to_string());
    }
    let name = filename.trim();

    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "非法的备份文件名：{name}（不允许包含路径分隔符）"
        ));
    }
    if name == "." || name == ".." || name.contains("..") {
        return Err(format!("非法的备份文件名：{name}（不允许相对路径段）"));
    }
    // `C:` 之类盘符前缀，以及 NUL / 控制字符。
    if name.contains(':') || name.contains('\0') {
        return Err(format!("非法的备份文件名：{name}（不允许盘符或控制字符）"));
    }
    if name.chars().any(|c| (c as u32) < 0x20) {
        return Err(format!("非法的备份文件名：{name}（包含控制字符）"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 本地文件原语
// ---------------------------------------------------------------------------

/// 原子写：先写同目录临时文件，再 rename 覆盖。
///
/// rename 在同一文件系统内是原子的，因此读者要么看到旧内容、
/// 要么看到完整的新内容，不会看到「被截断了一半」的中间态。
/// 这正是任务书 P0-1 任务 E 要求的「恢复期间崩溃不留半成品」。
pub fn atomic_write(target: &Path, data: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("目标路径没有父目录：{}", target.display()))?;
    fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;

    let tmp = parent.join(format!(
        ".{}.tmp_{}",
        target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "out".to_string()),
        uuid::Uuid::new_v4()
    ));

    fs::write(&tmp, data).map_err(|e| format!("写入临时文件失败: {e}"))?;
    fs::rename(&tmp, target).map_err(|e| {
        // 失败时不要留下临时文件
        let _ = fs::remove_file(&tmp);
        format!("原子替换失败（{}）: {e}", target.display())
    })
}

pub fn copy_database(src: &Path, dst: &Path) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|e| format!("读取源数据库失败: {e}"))?;
    atomic_write(dst, &bytes)
}

/// 用 SQLite Backup API 生成一致性快照。
///
/// 该 API 在 WAL 模式下同样是原子的：它会先把源库的全部页面复制到一个
/// 临时页库再逐页写入目标，因此不会产生「一半新一半旧」的快照，
/// 也不要求源库此刻没有其他连接。
pub fn snapshot_to_file(pool: &DbPool, target: &Path) -> Result<(), String> {
    let active = pool.get().map_err(|e| format!("获取数据库连接失败: {e}"))?;
    snapshot_with(&active, target)
}

pub fn snapshot_with(active: &rusqlite::Connection, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建快照目录失败: {e}"))?;
    }

    // 快照先落临时路径再 rename：崩溃时不会留下一个半截的 .db 被当成有效备份。
    let tmp = target.with_extension(format!(
        "tmp_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")
    ));

    {
        let mut temp_conn = rusqlite::Connection::open(&tmp)
            .map_err(|e| format!("创建临时快照数据库失败: {e}"))?;
        let backup = rusqlite::backup::Backup::new(active, &mut temp_conn)
            .map_err(|e| format!("初始化 SQLite 备份失败: {e}"))?;
        backup
            .run_to_completion(20, std::time::Duration::from_millis(20), None)
            .map_err(|e| {
                let _ = fs::remove_file(&tmp);
                format!("执行 SQLite 一致性快照失败: {e}")
            })?;
    }

    fs::rename(&tmp, target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("快照落盘失败: {e}")
    })
}

// ---------------------------------------------------------------------------
// 恢复前预检
// ---------------------------------------------------------------------------

/// 预检结论。字段刻意保持扁平，方便直接透传给前端展示。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PreflightReport {
    /// 结构上是否是一个可用的备份（不代表内容「一定」正确）。
    pub is_valid: bool,
    /// 失败原因；`is_valid` 为 true 时为空。
    pub reason: String,
    pub schema_version: i64,
    pub novel_count: i64,
    pub volume_count: i64,
    pub chapter_count: i64,
    pub word_count: i64,
    /// 库结构完整但一条作品都没有。
    ///
    /// 单独标记而不是直接判失败：空库可能是用户真的清空过，
    /// 也可能是备份被清空。**是否恢复由作者决定，程序不替他决定。**
    pub is_empty_library: bool,
}

/// 恢复前的强制检查。任何一项不过都必须拒绝覆盖现有数据库。
pub fn preflight_backup(path: &Path) -> Result<PreflightReport, String> {
    let mut report = PreflightReport {
        is_valid: false,
        reason: String::new(),
        schema_version: 0,
        novel_count: 0,
        volume_count: 0,
        chapter_count: 0,
        word_count: 0,
        is_empty_library: false,
    };

    let conn = match rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(e) => {
            // 打不开基本都是「这根本不是 SQLite 文件」——
            // 截断的下载、被替换的 HTML 错误页、非数据库文件都会走到这里。
            return Err(format!("备份不是有效的 SQLite 数据库：{e}"));
        }
    };

    // 完整性检查：截断 / 页损坏的库会在这里暴露。
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| format!("数据库完整性检查失败：{e}"))?;
    if integrity != "ok" {
        return Err(format!("数据库完整性检查未通过：{integrity}"));
    }

    // schema_version 读不到必须报错。
    // 原实现在这里用 unwrap_or(0)，等于把「无法识别的文件」当成
    // 「schema v0 的合法旧库」放行 —— 这正是损坏备份能覆盖可用库的路径。
    report.schema_version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| format!("备份缺少 schema_version 表，无法确认格式版本：{e}"))?;

    if report.schema_version > crate::db::BASELINE_VERSION {
        return Err(format!(
            "该备份由更高版本的 Easy-writing（schema v{}）生成，当前客户端仅支持到 v{}，请先升级软件。",
            report.schema_version,
            crate::db::BASELINE_VERSION
        ));
    }

    report.novel_count = count(&conn, "novel")?;
    report.volume_count = count(&conn, "volume")?;
    report.chapter_count = count(&conn, "chapter")?;
    report.word_count = conn
        .query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM chapter",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    report.is_empty_library = report.novel_count == 0;
    report.is_valid = true;
    Ok(report)
}

fn count(conn: &rusqlite::Connection, table: &str) -> Result<i64, String> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
        r.get::<_, i64>(0)
    })
    .map_err(|e| format!("统计 {table} 失败，备份结构可能不完整：{e}"))
}

// ---------------------------------------------------------------------------
// 轮转
// ---------------------------------------------------------------------------

/// 计算本次应当删除的旧备份文件名。
///
/// 两条硬规则：
/// 1. **新备份未验证通过时，一个都不删** —— 否则「新备份不可用 + 旧备份已删」
///    就是不可恢复的组合（基线 C-03）。
/// 2. **只处理本应用创建的 `backup_` 前缀文件** —— 用户手动放进远端目录的
///    其它 `.db.gz` 不属于本应用的备份策略范围，不能被顺手删掉。
///
/// `names` 应当是远端目录下全部 `.db.gz` 文件名，顺序不限。
pub fn plan_rotation(names: &[String], new_backup_verified: bool, keep: usize) -> Vec<String> {
    if !new_backup_verified {
        return Vec::new();
    }
    let mut managed: Vec<&String> = names
        .iter()
        .filter(|n| {
            let base = n.strip_suffix(".json").unwrap_or(n);
            base.starts_with("backup_") && base.ends_with(".db.gz")
        })
        .collect();

    // 按文件名排序即按时间戳排序（备份名内嵌 UTC 时间戳）。
    managed.sort();
    if managed.len() <= keep {
        return Vec::new();
    }
    managed[..managed.len() - keep]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// 增量备份主流程
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOutcome {
    pub filename: String,
    /// 本次备份是否已通过结构自检。轮转只在此为 true 时执行。
    pub verified: bool,
    /// 清单 sidecar 是否上传成功。数据库备份本体不受它影响。
    pub manifest_uploaded: bool,
    /// 未同步成功的图片文件名。数据库本体不受影响，但作者应知道素材不完整。
    pub image_upload_failures: Vec<String>,
    /// 数据库 `asset` 表有记录、但备份时本地文件已缺失的素材 `rel_path`。
    ///
    /// 与 `image_upload_failures` 不同：那些是「文件在、上传失败」，
    /// 这些是「文件本来就不在」。清单会把它们记进 `missing_assets`，
    /// 恢复时必须能明确告诉作者哪些素材从一开始就没进备份。
    pub missing_assets: Vec<String>,
}

/// 生成快照 → 校验 → 上传 → 落清单 → 才轮转。
///
/// 与旧实现的顺序差异：旧实现在上传后**从不回读校验**就进入轮转；
/// 现在上传完成后会对远端做一次大小比对，并对本地快照做一次结构自检，
/// 两者都通过才允许删除旧备份。
pub async fn run_incremental_backup(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    app_data_dir: PathBuf,
) -> Result<String, String> {
    let outcome = run_incremental_backup_detailed(pool, client, remote_dir, app_data_dir).await?;
    Ok(outcome.filename)
}

pub async fn run_incremental_backup_detailed(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    app_data_dir: PathBuf,
) -> Result<BackupOutcome, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    let images_remote_dir = format!("{}/images", clean_dir);

    // 1. Ensure remote directories exist
    client.ensure_dir(&clean_dir).await?;
    client.ensure_dir(&images_remote_dir).await?;

    // 2. Incremental Image Upload
    let local_images_dir = app_data_dir.join("images");
    let mut image_upload_failures: Vec<String> = Vec::new();
    if local_images_dir.exists() && local_images_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&local_images_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        let remote_image_path = format!("{}/{}", images_remote_dir, filename);
                        // Check if already uploaded
                        if let Ok(false) = client.file_exists(&remote_image_path).await {
                            if let Ok(img_bytes) = fs::read(&path) {
                                if let Err(e) =
                                    client.upload_file(&remote_image_path, img_bytes).await
                                {
                                    // 旧实现 `let _ =` 把失败完全吞掉。
                                    // 单张图传失败不该毁掉整次备份（数据库本体才是关键），
                                    // 但必须让作者知道图片没有全部同步。
                                    image_upload_failures.push(filename.to_string());
                                    eprintln!("[backup] 图片上传失败 {filename}: {e}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Zero-locking SQLite Snapshot
    let temp_dir = std::env::temp_dir();
    let temp_db_path = temp_dir.join(format!(
        "easy_writing_snapshot_{}.db",
        uuid::Uuid::new_v4()
    ));
    snapshot_to_file(&pool, &temp_db_path)?;

    // 4. **先自检，再上传。** 这一步是旧实现完全缺失的：
    // 旧代码把快照压缩后直接上传，从不确认这份快照本身是不是可用的库。
    let preflight = match preflight_backup(&temp_db_path) {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_file(&temp_db_path);
            return Err(format!("备份快照自检未通过，已放弃上传：{e}"));
        }
    };

    // 5. Gzip Compression
    let raw_db_bytes = fs::read(&temp_db_path)
        .map_err(|e| format!("读取快照数据库失败: {e}"))?;
    let _ = fs::remove_file(&temp_db_path);

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&raw_db_bytes)
        .map_err(|e| format!("Gzip 压缩失败: {e}"))?;
    let compressed_bytes = encoder
        .finish()
        .map_err(|e| format!("完成 Gzip 压缩流失败: {e}"))?;

    let compressed_len = compressed_bytes.len() as u64;
    let compressed_hash = manifest::sha256_hex(&compressed_bytes);

    // 6. Build manifest (sidecar). 不含任何凭据。
    //
    //    清单生成失败必须让整次备份失败：一份计数全为 0 的伪造清单
    //    比没有清单更糟 —— 它会让恢复时的交叉校验「通过」。
    //    同时，备份时本地已缺失的素材必须回报，不能因为
    //    reconcile_with_disk 把它们从清单里剔掉就当没发生。
    let (manifest_json, missing_assets) = {
        let conn = pool.get().map_err(|e| format!("获取数据库连接失败: {e}"))?;
        let mut m = manifest::build_manifest(
            &conn,
            env!("CARGO_PKG_VERSION"),
            compressed_len,
            &compressed_hash,
        )
        .map_err(|e| format!("生成备份清单失败，已放弃上传：{e}"))?;
        let missing = manifest::reconcile_with_disk(&mut m, &app_data_dir);
        if !missing.is_empty() {
            eprintln!(
                "[backup] {} 个素材文件在本地缺失，未进入本次备份",
                missing.len()
            );
        }
        (
            serde_json::to_vec_pretty(&m).map_err(|e| format!("序列化备份清单失败: {e}"))?,
            missing,
        )
    };

    // 7. Atomic Upload to WebDAV
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%SZ").to_string();
    let backup_filename = format!("backup_{}.db.gz", timestamp);
    let tmp_backup_filename = format!("{}.tmp", backup_filename);
    let remote_tmp_path = format!("{}/{}", clean_dir, tmp_backup_filename);
    let remote_backup_path = format!("{}/{}", clean_dir, backup_filename);
    let remote_manifest_path = format!("{}.json", remote_backup_path);

    // Upload to .tmp first
    client
        .upload_file(&remote_tmp_path, compressed_bytes)
        .await?;

    // Verify remote size **if the server supports it**.
    // 旧实现用 `if let Ok(..)` 把「服务端不支持 HEAD」和「校验通过」混为一谈：
    // 不支持时整段校验被静默跳过，却仍然当作已校验。这里区分这两种情况。
    let mut size_verified = false;
    match client.get_file_size(&remote_tmp_path).await {
        Ok(remote_size) if remote_size > 0 => {
            if remote_size != compressed_len {
                let _ = client.delete_file(&remote_tmp_path).await;
                return Err(format!(
                    "上传校验失败：本地压缩大小 ({} bytes) 与远程文件大小 ({} bytes) 不一致",
                    compressed_len, remote_size
                ));
            }
            size_verified = true;
        }
        Ok(_) => {
            // 服务端返回 0 / 无 Content-Length：无法确认，但也不能算失败。
        }
        Err(e) => {
            let _ = client.delete_file(&remote_tmp_path).await;
            return Err(format!("无法确认远端上传结果，已放弃本次备份：{e}"));
        }
    }

    // Atomically rename .tmp to destination .db.gz
    if let Err(e) = client.move_file(&remote_tmp_path, &remote_backup_path).await {
        let _ = client.delete_file(&remote_tmp_path).await;
        return Err(format!("重命名快照备份失败: {e}"));
    }

    // 清单上传失败**不影响**数据库备份本身的可用性，但必须告知，
    // 否则用户会以为「这份备份是有清单的」。
    let manifest_uploaded = match client
        .upload_file(&remote_manifest_path, manifest_json)
        .await
    {
        Ok(()) => true,
        Err(e) => {
            eprintln!("[backup] 清单上传失败（备份本体仍有效）: {e}");
            false
        }
    };

    // 8. 只有确认落盘成功才允许轮转。
    //    verified 的含义是「这份备份我已经确认是完整可用的」。
    let verified = size_verified && preflight.is_valid;

    prune_remote_backups(client, &clean_dir, verified).await?;

    Ok(BackupOutcome {
        filename: backup_filename,
        verified,
        manifest_uploaded,
        image_upload_failures,
        missing_assets,
    })
}

/// 轮转远端备份，返回实际删除成功的数量。
///
/// 删除失败会向上冒泡（不再 `let _ =` 吞掉），让调用方知道
/// 「远端正在无限增长」这件事必须有人处理。
async fn prune_remote_backups(
    client: &WebDavClient,
    clean_dir: &str,
    new_backup_verified: bool,
) -> Result<usize, String> {
    const KEEP: usize = 20;

    let backups = client.list_files(clean_dir).await?;
    let names: Vec<String> = backups.into_iter().map(|b| b.filename).collect();
    let to_delete = plan_rotation(&names, new_backup_verified, KEEP);
    if to_delete.is_empty() {
        return Ok(0);
    }

    let mut deleted = 0usize;
    let mut failures = Vec::new();
    for name in to_delete {
        let path = format!("{}/{}", clean_dir, name);
        match client.delete_file(&path).await {
            Ok(()) => deleted += 1,
            // 404 已被 delete_file 视为成功，这里拿到的是真失败
            Err(e) => failures.push(format!("{name}: {e}")),
        }
        // 清单 sidecar 必须跟着一起删。
        //
        // `list_files` 只返回 `.db.gz`，因此 sidecar 从不出现在 `names` 里，
        // `plan_rotation` 也就永远看不到它 —— 少了这一行，每次轮转都会在
        // 用户的 WebDAV 上留下一份孤儿 `.json`，日积月累无人清理。
        // delete_file 把 404 视为成功，所以历史无清单备份不受影响。
        let sidecar = format!("{path}.json");
        if let Err(e) = client.delete_file(&sidecar).await {
            failures.push(format!("{name}.json: {e}"));
        }
    }
    if !failures.is_empty() {
        // 不静默：轮转失败意味着远端目录会持续增长，需要让用户知道。
        return Err(format!(
            "已删除 {} 个旧备份，但有 {} 个删除失败（远端目录会持续增长）：\n{}",
            deleted,
            failures.len(),
            failures.join("\n")
        ));
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn filename_validation_rejects_traversal() {
        assert!(validate_backup_filename("backup_20260101_000000Z.db.gz").is_ok());
        assert!(validate_backup_filename("../../../etc/passwd").is_err());
        assert!(validate_backup_filename("a/b.db.gz").is_err());
        assert!(validate_backup_filename("a\\b.db.gz").is_err());
        assert!(validate_backup_filename("").is_err());
        assert!(validate_backup_filename("   ").is_err());
        assert!(validate_backup_filename("C:evil.db.gz").is_err());
    }

    #[test]
    fn rotation_keeps_n_when_under_limit() {
        let list = names(&["backup_20260101_000000Z.db.gz", "backup_20260102_000000Z.db.gz"]);
        assert!(plan_rotation(&list, true, 20).is_empty());
    }

    #[test]
    fn rotation_deletes_oldest_first() {
        let list = names(&[
            "backup_20260101_000000Z.db.gz",
            "backup_20260102_000000Z.db.gz",
            "backup_20260103_000000Z.db.gz",
        ]);
        let del = plan_rotation(&list, true, 2);
        assert_eq!(del, vec!["backup_20260101_000000Z.db.gz".to_string()]);
    }

    #[test]
    fn rotation_is_noop_when_unverified() {
        let list: Vec<String> = (1..=25)
            .map(|i| format!("backup_202601{:02}_000000Z.db.gz", i))
            .collect();
        assert!(plan_rotation(&list, false, 20).is_empty());
    }

    #[test]
    fn rotation_ignores_foreign_files() {
        let list = names(&[
            "backup_20260103_000000Z.db.gz",
            "backup_20260102_000000Z.db.gz",
            "backup_20260101_000000Z.db.gz",
            "someone-elses-data.db.gz",
        ]);
        let del = plan_rotation(&list, true, 2);
        assert_eq!(del.len(), 1);
        assert!(!del.iter().any(|n| n.contains("someone")));
    }

    #[test]
    fn preflight_detects_missing_sqlite_header() {
        let dir = std::env::temp_dir().join(format!("ew_pf_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.db");
        std::fs::write(&f, b"definitely not sqlite").unwrap();
        assert!(preflight_backup(&f).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

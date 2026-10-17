use crate::db::DbPool;
use crate::sync::webdav::WebDavClient;
use chrono::Local;
use flate2::read::GzDecoder;
use shared_types::BackupFileInfo;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

pub async fn list_remote_backups(
    client: &WebDavClient,
    remote_dir: &str,
) -> Result<Vec<BackupFileInfo>, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    client.list_files(&clean_dir).await
}

/// 恢复过程中发现的单个不一致项。
///
/// 存在的意义：旧实现把这些情况全部 `let _ =` 吞掉，用户看到的是"恢复成功"，
/// 实际素材可能缺失或被篡改。现在必须带回前端展示。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RestoreIssue {
    pub filename: String,
    pub reason: String,
}

/// 恢复结果报告。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RestoreReport {
    pub issues: Vec<RestoreIssue>,
}

pub async fn run_consistency_restore(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    backup_filename: &str,
    app_data_dir: PathBuf,
) -> Result<RestoreReport, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    let remote_backup_path = format!("{}/{}", clean_dir, backup_filename);

    // 1. Local Safety Fallback Snapshot
    let local_backups_dir = app_data_dir.join("backups");
    let _ = fs::create_dir_all(&local_backups_dir);

    let pre_restore_filename = format!(
        "pre_restore_{}.db",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let pre_restore_path = local_backups_dir.join(pre_restore_filename);

    {
        let active_conn = pool.get().map_err(|e| format!("获取当前数据库连接失败: {}", e))?;
        let mut pre_restore_conn = rusqlite::Connection::open(&pre_restore_path)
            .map_err(|e| format!("创建本地防覆写快照数据库失败: {}", e))?;

        let backup = rusqlite::backup::Backup::new(&active_conn, &mut pre_restore_conn)
            .map_err(|e| format!("创建防覆写快照失败: {}", e))?;

        backup
            .run_to_completion(20, std::time::Duration::from_millis(20), None)
            .map_err(|e| format!("执行防覆写快照失败: {}", e))?;
    }

    // 2. Download remote backup .db.gz
    let gz_bytes = client.download_file(&remote_backup_path).await?;

    // 3. Gunzip decompression
    let mut decoder = GzDecoder::new(&gz_bytes[..]);
    let mut decompressed_db_bytes = Vec::new();
    decoder
        .read_to_end(&mut decompressed_db_bytes)
        .map_err(|e| format!("解压 WebDAV 备份包失败: {}", e))?;

    // 4. Save to temporary file and restore into active DB using SQLite Backup API
    let temp_dir = std::env::temp_dir();
    let temp_restored_path = temp_dir.join(format!("easy_writing_restored_{}.db", uuid::Uuid::new_v4()));
    fs::write(&temp_restored_path, decompressed_db_bytes)
        .map_err(|e| format!("写入解压临时文件失败: {}", e))?;

    {
        let src_conn = rusqlite::Connection::open(&temp_restored_path)
            .map_err(|e| format!("打开解压数据库失败: {}", e))?;

        let mut target_conn = pool.get().map_err(|e| format!("获取目标数据库连接失败: {}", e))?;

        let backup = rusqlite::backup::Backup::new(&src_conn, &mut target_conn)
            .map_err(|e| format!("初始化还原管道失败: {}", e))?;

        backup
            .run_to_completion(20, std::time::Duration::from_millis(20), None)
            .map_err(|e| format!("还原数据至目标数据库失败: {}", e))?;
    }

    let _ = fs::remove_file(&temp_restored_path);

    // 5. Scan database for missing images and backfill from WebDAV
    let local_images_dir = app_data_dir.join("images");
    let _ = fs::create_dir_all(&local_images_dir);

    let images_remote_dir = format!("{}/images", clean_dir);

    // Query asset table for all recorded hashes
    let asset_rows: Vec<(String, String)> = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT rel_path, hash FROM asset")
            .map_err(|e| e.to_string())?;

        let rows: Result<Vec<_>, _> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect();
        rows.map_err(|e| e.to_string())?
    };

    // 回填缺失图片，并**校验内容**。
    //
    // 旧实现把 SELECT 出来的 hash 直接丢掉（for (.., _hash)），只按文件名判断存在性，
    // 下载失败也是 let _ = 吞掉。结果是"恢复完成"可能意味着素材其实缺失或已被篡改，
    // 而用户完全看不出来。现在逐项校验并把问题带回上层。
    let mut issues: Vec<RestoreIssue> = Vec::new();

    for (file_path_str, hash) in asset_rows {
        let p = PathBuf::from(&file_path_str);
        let Some(filename) = p.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            issues.push(RestoreIssue {
                filename: file_path_str.clone(),
                reason: "资产 rel_path 没有文件名，无法定位本地文件".to_string(),
            });
            continue;
        };

        let target_local_image = local_images_dir.join(&filename);
        let remote_img_path = format!("{}/{}", images_remote_dir, filename);

        // 本地已存在也要校验：备份可能是另一台机器的，同名不同内容会静默错配。
        if target_local_image.exists() {
            match fs::read(&target_local_image) {
                Ok(local) if sha256_hex(&local) == hash => continue,
                Ok(_) => {
                    issues.push(RestoreIssue {
                        filename: filename.clone(),
                        reason: "本地已有同名文件但内容与备份记录不符，已用备份覆盖".to_string(),
                    });
                }
                Err(e) => {
                    issues.push(RestoreIssue {
                        filename: filename.clone(),
                        reason: format!("读取本地文件失败，已用备份覆盖：{}", e),
                    });
                }
            }
        }

        match client.download_file(&remote_img_path).await {
            Ok(img_data) => {
                let actual = sha256_hex(&img_data);
                if actual != hash {
                    issues.push(RestoreIssue {
                        filename: filename.clone(),
                        reason: format!(
                            "远端图片哈希不匹配（期望 {}，实际 {}），已拒绝写入",
                            short_hash(&hash),
                            short_hash(&actual)
                        ),
                    });
                    continue;
                }
                if let Err(e) = fs::write(&target_local_image, &img_data) {
                    issues.push(RestoreIssue {
                        filename: filename.clone(),
                        reason: format!("写入本地失败：{}", e),
                    });
                }
            }
            Err(e) => {
                issues.push(RestoreIssue {
                    filename: filename.clone(),
                    reason: format!("远端下载失败：{}", e),
                });
            }
        }
    }

    Ok(RestoreReport { issues })
}

/// 计算与 asset.rs 保存素材时一致的 SHA-256 十六进制摘要。
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

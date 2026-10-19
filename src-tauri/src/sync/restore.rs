use crate::db::DbPool;
use crate::sync::webdav::WebDavClient;
use chrono::Local;
use flate2::read::GzDecoder;
pub use shared_types::{BackupFileInfo, DanglingKeyInfo, RestoreIssue, RestoreReport};
use std::fs;
use std::path::PathBuf;

pub async fn list_remote_backups(
    client: &WebDavClient,
    remote_dir: &str,
) -> Result<Vec<BackupFileInfo>, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    client.list_files(&clean_dir).await
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

    // 暂存本地独有的连接配置与机器相关设置，避免被远端全库覆盖
    let preserved_webdav: Option<(String, String, String, String, String)> = {
        let active_conn = pool.get().map_err(|e| e.to_string())?;
        active_conn
            .query_row(
                "SELECT id, url, username, pwd_ref, remote_dir FROM webdav_config LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .ok()
    };

    let preserved_settings: Vec<(String, String)> = {
        let active_conn = pool.get().map_err(|e| e.to_string())?;
        let mut stmt = active_conn
            .prepare("SELECT key, value FROM app_setting WHERE key IN ('theme', 'left_panel_visible', 'right_panel_visible')")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        rows
    };

    // 2. Download remote backup .db.gz
    let gz_bytes = client.download_file(&remote_backup_path).await?;

    // 3. Gunzip decompression
    //
    //    上限保护：一个 .gz 的解压体积可以远超其自身大小（压缩炸弹）。
    //    取前 MAX+1 字节即可判断越界，不把整包读进内存。
    let mut decoder = GzDecoder::new(&gz_bytes[..]);
    let mut decompressed_db_bytes = Vec::new();
    // 分块读取并在超限时立即中止。
    {
        use std::io::Read;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0usize;
        loop {
            let n = decoder
                .read(&mut buf)
                .map_err(|e| format!("解压 WebDAV 备份包失败: {}", e))?;
            if n == 0 {
                break;
            }
            total += n;
            if total > crate::sync::backup::MAX_RESTORE_DECOMPRESSED_BYTES {
                return Err(format!(
                    "备份解压后超过 {} MiB 上限，已中止恢复（可能是损坏文件或压缩炸弹）。",
                    crate::sync::backup::MAX_RESTORE_DECOMPRESSED_BYTES / (1024 * 1024)
                ));
            }
            decompressed_db_bytes.extend_from_slice(&buf[..n]);
        }
    }

    // 4. Save to temporary file and restore into active DB using SQLite Backup API
    let temp_dir = std::env::temp_dir();
    let temp_restored_path = temp_dir.join(format!("easy_writing_restored_{}.db", uuid::Uuid::new_v4()));
    fs::write(&temp_restored_path, decompressed_db_bytes)
        .map_err(|e| format!("写入解压临时文件失败: {}", e))?;

    // 4a. **预检门槛** —— 覆盖现有数据库之前必须先证明这份备份是好的。
    //
    //     这是本次 V0.3.8 新增的关键防线。旧实现在这里直接
    //     `Backup::run_to_completion` 覆盖目标库，只用
    //     `schema_version > BASELINE_VERSION` 一项做检查，
    //     而该项在读取失败时 `unwrap_or(0)` —— 损坏/非 SQLite 文件
    //     会被当成 v0 合法旧库放行并覆盖用户数据。
    let preflight = match crate::sync::backup::preflight_backup(&temp_restored_path) {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_file(&temp_restored_path);
            return Err(format!("恢复中止：备份未通过完整性检查，原数据未被改动。{e}"));
        }
    };

    // 空库单独提示，但不自动拒绝 —— 作者可能确实要恢复到空白状态。
    let manifest_notes = crate::sync::backup::cross_check_manifest(&temp_restored_path, &preflight)
        .unwrap_or_else(|e| vec![format!("读取备份清单时出错：{e}")]);

    {
        let src_conn = rusqlite::Connection::open(&temp_restored_path)
            .map_err(|e| format!("打开解压数据库失败: {}", e))?;

        // 高版本检查已由上面的 preflight_backup 承担（它读 schema_version
        // 失败会直接报错，而不是 unwrap_or(0) 伪装成 v0 合法旧库）。

        let mut target_conn = pool.get().map_err(|e| format!("获取目标数据库连接失败: {}", e))?;

        let backup = rusqlite::backup::Backup::new(&src_conn, &mut target_conn)
            .map_err(|e| format!("初始化还原管道失败: {}", e))?;

        backup
            .run_to_completion(20, std::time::Duration::from_millis(20), None)
            .map_err(|e| format!("还原数据至目标数据库失败: {}", e))?;
    }

    let _ = fs::remove_file(&temp_restored_path);

    // 恢复本地暂存的核心配置（WebDAV 连接凭据与机器外观设置）
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        if let Some((id, url, username, pwd_ref, remote_dir)) = preserved_webdav {
            let _ = conn.execute(
                "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
                 ON CONFLICT(id) DO UPDATE SET
                   url = excluded.url,
                   username = excluded.username,
                   pwd_ref = excluded.pwd_ref,
                   remote_dir = excluded.remote_dir,
                   updated_at = excluded.updated_at",
                rusqlite::params![id, url, username, pwd_ref, remote_dir],
            );
        }
        for (k, v) in preserved_settings {
            let _ = conn.execute(
                "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![k, v],
            );
        }
    }

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

    // 恢复摘要：作者在点下「恢复」之后需要立刻知道恢复了什么。
    issues.push(RestoreIssue {
        filename: "（恢复摘要）".to_string(),
        reason: format!(
            "已恢复到备份时的状态：schema v{}，{} 部作品 / {} 卷 / {} 章 / {} 字。{}",
            preflight.schema_version,
            preflight.novel_count,
            preflight.volume_count,
            preflight.chapter_count,
            preflight.word_count,
            manifest_notes.join(" ")
        ),
    });
    if preflight.is_empty_library {
        issues.push(RestoreIssue {
            filename: "（恢复摘要）".to_string(),
            reason: "注意：这份备份里一部作品都没有。如果你原本有作品，说明这份备份是空库或已损坏。".to_string(),
        });
    }

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
                if let Err(e) = crate::sync::backup::atomic_write(&target_local_image, &img_data)
                {
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

    // 6. 钥匙串 key_ref 有效性体检：检测哪些 profile 恢复后在本机缺乏密钥
    let profiles: Vec<(String, String, String)> = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, name, key_ref FROM llm_profile")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        rows
    };

    let mut dangling_keys = Vec::new();
    for (p_id, p_name, key_ref) in profiles {
        if !key_ref.is_empty() {
            if crate::keyring::get_secret(&key_ref).is_err() {
                dangling_keys.push(DanglingKeyInfo {
                    profile_id: p_id,
                    profile_name: p_name,
                    key_ref,
                });
            }
        }
    }

    Ok(RestoreReport { issues, dangling_keys })
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

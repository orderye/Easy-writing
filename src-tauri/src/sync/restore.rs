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

pub async fn run_consistency_restore(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    backup_filename: &str,
    app_data_dir: PathBuf,
) -> Result<(), String> {
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

    for (file_path_str, _hash) in asset_rows {
        let p = PathBuf::from(&file_path_str);
        if let Some(filename) = p.file_name().and_then(|n| n.to_str()) {
            let target_local_image = local_images_dir.join(filename);
            if !target_local_image.exists() {
                // Download from WebDAV
                let remote_img_path = format!("{}/{}", images_remote_dir, filename);
                if let Ok(img_data) = client.download_file(&remote_img_path).await {
                    let _ = fs::write(&target_local_image, img_data);
                }
            }
        }
    }

    Ok(())
}

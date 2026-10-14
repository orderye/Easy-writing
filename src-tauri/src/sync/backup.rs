use crate::db::DbPool;
use crate::sync::webdav::WebDavClient;
use chrono::Local;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub async fn run_incremental_backup(
    pool: DbPool,
    client: &WebDavClient,
    remote_dir: &str,
    app_data_dir: PathBuf,
) -> Result<String, String> {
    let clean_dir = remote_dir.trim_matches('/').to_string();
    let images_remote_dir = format!("{}/images", clean_dir);

    // 1. Ensure remote directories exist
    client.ensure_dir(&clean_dir).await?;
    client.ensure_dir(&images_remote_dir).await?;

    // 2. Incremental Image Upload
    let local_images_dir = app_data_dir.join("images");
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
                                let _ = client.upload_file(&remote_image_path, img_bytes).await;
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Zero-locking SQLite Snapshot
    let temp_dir = std::env::temp_dir();
    let temp_db_filename = format!("easy_writing_snapshot_{}.db", uuid::Uuid::new_v4());
    let temp_db_path = temp_dir.join(&temp_db_filename);

    {
        let active_conn = pool.get().map_err(|e| format!("获取数据库连接失败: {}", e))?;
        let mut temp_conn = rusqlite::Connection::open(&temp_db_path)
            .map_err(|e| format!("创建临时快照数据库失败: {}", e))?;

        let backup = rusqlite::backup::Backup::new(&active_conn, &mut temp_conn)
            .map_err(|e| format!("初始化 SQLite 备份失败: {}", e))?;

        backup
            .run_to_completion(20, std::time::Duration::from_millis(20), None)
            .map_err(|e| format!("执行 SQLite 一致性快照失败: {}", e))?;
    }

    // 4. Gzip Compression
    let raw_db_bytes = fs::read(&temp_db_path)
        .map_err(|e| format!("读取快照数据库失败: {}", e))?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&raw_db_bytes)
        .map_err(|e| format!("Gzip 压缩失败: {}", e))?;
    let compressed_bytes = encoder
        .finish()
        .map_err(|e| format!("完成 Gzip 压缩流失败: {}", e))?;

    // 5. Atomic Upload to WebDAV
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_filename = format!("backup_{}.db.gz", timestamp);
    let remote_backup_path = format!("{}/{}", clean_dir, backup_filename);

    client
        .upload_file(&remote_backup_path, compressed_bytes)
        .await?;

    // 6. Clean up temporary snapshot file
    let _ = fs::remove_file(temp_db_path);

    Ok(backup_filename)
}

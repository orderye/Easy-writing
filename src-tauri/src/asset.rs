use base64::Engine;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub fn save_image(
    conn: &Connection,
    app_data_dir: &Path,
    novel_id: &str,
    name: &str,
    mime: &str,
    base64_data: &str,
) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash_hex = format!("{:x}", hasher.finalize());

    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    };

    let filename = format!("{}.{}", hash_hex, ext);
    let images_dir = app_data_dir.join("images");
    fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;

    let target_path = images_dir.join(&filename);
    if !target_path.exists() {
        fs::write(&target_path, &bytes).map_err(|e| e.to_string())?;
    }

    let rel_path = format!("images/{}", filename);
    let asset_id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO asset (id, novel_id, name, mime, rel_path, size, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            asset_id,
            novel_id,
            name,
            mime,
            rel_path,
            bytes.len() as i64,
            hash_hex
        ],
    )
    .map_err(|e| e.to_string())?;

    Ok(rel_path)
}

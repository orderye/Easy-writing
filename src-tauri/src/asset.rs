use base64::Engine;
use rusqlite::{params, Connection};
use shared_types::AssetInfo;
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

/// 列出某作品的图片资产，按创建时间倒序。
pub fn list_assets(conn: &Connection, novel_id: &str) -> Result<Vec<AssetInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, novel_id, name, mime, rel_path, size, hash, created_at
             FROM asset
             WHERE novel_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![novel_id], |row| {
            Ok(AssetInfo {
                id: row.get(0)?,
                novel_id: row.get(1)?,
                name: row.get(2)?,
                mime: row.get(3)?,
                rel_path: row.get(4)?,
                size: row.get(5)?,
                hash: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r.map_err(|e| e.to_string())?);
    }
    Ok(list)
}

//! 图片资产（编辑器插图 / 图库 / WebDAV 图片链路）。
//!
//! 【三条约束，下面每个函数都据此设计】
//! 1. 文件按内容哈希命名且同内容只写一次，而每次上传都插一行 ——
//!    「一条记录」不等于「一个文件」。任何删除都必须先数同 rel_path 的其余记录。
//! 2. 正文与历史版本里存的是含绝对路径的 asset URL。URL 编码后
//!    「images/xxx.png」不一定原样出现，但 64 位十六进制哈希必然原样出现，
//!    因此引用检查按 hash 匹配，而不是按 rel_path。
//! 3. asset 表是 WebDAV 恢复的补图清单（sync/restore.rs 逐行补图），所以本模块的
//!    删除一律只软删记录、不动磁盘文件；文件回收交给图库「维护」里的孤儿清理，
//!    那里会先确认该文件既无记录引用、也无正文引用。

use base64::Engine;
use rusqlite::{params, Connection};
use shared_types::{AssetInfo, ImageAssetUsage};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::db::mapper::{query_list, query_optional};

/// 资产列清单：列序与 map_asset 严格对应（改 SELECT 时两处一起改）。
const ASSET_COLS: &str =
    "id, novel_id, name, mime, rel_path, size, hash, created_at, deleted_at";

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

fn map_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetInfo> {
    Ok(AssetInfo {
        id: row.get(0)?,
        novel_id: row.get(1)?,
        name: row.get(2)?,
        mime: row.get(3)?,
        rel_path: row.get(4)?,
        size: row.get(5)?,
        hash: row.get(6)?,
        created_at: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

/// 列出某作品**在库**的图片资产，按创建时间倒序（同秒内按写入顺序倒序）。
pub fn list_assets(conn: &Connection, novel_id: &str) -> Result<Vec<AssetInfo>, String> {
    query_list(
        conn,
        &format!(
            "SELECT {} FROM asset
             WHERE novel_id = ?1 AND deleted_at IS NULL
             ORDER BY created_at DESC, rowid DESC",
            ASSET_COLS
        ),
        params![novel_id],
        map_asset,
    )
    .map_err(|e| e.to_string())
}

/// 列出某作品**已移除**的图片资产（软删除，可恢复），最近移除的在前。
pub fn list_deleted_assets(conn: &Connection, novel_id: &str) -> Result<Vec<AssetInfo>, String> {
    query_list(
        conn,
        &format!(
            "SELECT {} FROM asset
             WHERE novel_id = ?1 AND deleted_at IS NOT NULL
             ORDER BY deleted_at DESC, rowid DESC",
            ASSET_COLS
        ),
        params![novel_id],
        map_asset,
    )
    .map_err(|e| e.to_string())
}

/// 按 id 取一条记录（含已软删除的）。
pub fn get_asset(conn: &Connection, id: &str) -> Result<Option<AssetInfo>, String> {
    query_optional(
        conn,
        &format!("SELECT {} FROM asset WHERE id = ?1", ASSET_COLS),
        params![id],
        map_asset,
    )
    .map_err(|e| e.to_string())
}

/// 重命名资产：只改显示名。磁盘文件名是内容哈希，不受影响，
/// 因此既不影响正文引用，也不影响 WebDAV 的增量上传与补图。
pub fn rename_asset(conn: &Connection, id: &str, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("图片名称不能为空".to_string());
    }
    let affected = conn
        .execute(
            "UPDATE asset SET name = ?1 WHERE id = ?2",
            params![name, id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("图片不存在".to_string());
    }
    Ok(())
}

/// 从图库移除（软删除，可恢复）。**不动磁盘文件**，所以正文里的图仍然显示，
/// 只是不再出现在图库列表、也不再进入 WebDAV 的补图清单。
pub fn soft_delete_asset(conn: &Connection, id: &str) -> Result<(), String> {
    let affected = conn
        .execute(
            "UPDATE asset SET deleted_at = datetime('now')
             WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        // 要么不存在，要么已移除。前者报错，后者幂等成功。
        return match get_asset(conn, id)? {
            Some(_) => Ok(()),
            None => Err("图片不存在".to_string()),
        };
    }
    Ok(())
}

/// 恢复一张已移除的图片。
pub fn restore_asset(conn: &Connection, id: &str) -> Result<(), String> {
    let affected = conn
        .execute(
            "UPDATE asset SET deleted_at = NULL WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("图片不存在".to_string());
    }
    Ok(())
}

/// 彻底删除资产记录（不可恢复）。仅删记录：磁盘文件留给孤儿清理判断，
/// 因为「记录没了」不等于「文件没人用」（正文可能还引用着）。
pub fn purge_asset(conn: &Connection, id: &str) -> Result<(), String> {
    let affected = conn
        .execute("DELETE FROM asset WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("图片不存在".to_string());
    }
    Ok(())
}

/// 统计一张图的引用规模。删除/彻底删除前的风险提示就来自这里。
///
/// 按 hash 匹配而不是按 rel_path：正文里存的是含绝对路径的 asset URL，
/// 路径可能被百分号编码，而哈希是纯十六进制，编码前后都原样出现。
pub fn asset_usage(conn: &Connection, asset_id: &str) -> Result<ImageAssetUsage, String> {
    let Some(target) = get_asset(conn, asset_id)? else {
        return Err("图片不存在".to_string());
    };
    let like = format!("%{}%", target.hash);

    let referencing_chapters: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chapter WHERE content LIKE ?1",
            params![like],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let referencing_revisions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chapter_revision WHERE content LIKE ?1",
            params![like],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let shared_records: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM asset WHERE rel_path = ?1 AND id <> ?2",
            params![target.rel_path, asset_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(ImageAssetUsage {
        referencing_chapters,
        referencing_revisions,
        shared_records,
    })
}

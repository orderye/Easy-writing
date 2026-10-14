use rusqlite::{params, Connection, Result};
use shared_types::TrashItem;

pub fn list_trash_items(conn: &Connection, novel_id: &str) -> Result<Vec<TrashItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, 'volume' as item_type, title, summary as snippet, deleted_at 
         FROM volume 
         WHERE novel_id = ?1 AND deleted_at IS NOT NULL
         UNION ALL
         SELECT id, novel_id, 'chapter' as item_type, title, substr(content_plain, 1, 100) as snippet, deleted_at 
         FROM chapter 
         WHERE novel_id = ?1 AND deleted_at IS NOT NULL
         UNION ALL
         SELECT id, novel_id, 'worldview' as item_type, title, substr(content, 1, 100) as snippet, deleted_at 
         FROM worldview_entry 
         WHERE novel_id = ?1 AND deleted_at IS NOT NULL
         UNION ALL
         SELECT id, novel_id, 'character' as item_type, name as title, substr(profile, 1, 100) as snippet, deleted_at 
         FROM character 
         WHERE novel_id = ?1 AND deleted_at IS NOT NULL
         UNION ALL
         SELECT id, novel_id, 'plot' as item_type, title, substr(content, 1, 100) as snippet, deleted_at 
         FROM plot_item 
         WHERE novel_id = ?1 AND deleted_at IS NOT NULL
         ORDER BY deleted_at DESC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(TrashItem {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            item_type: row.get(2)?,
            title: row.get(3)?,
            snippet: row.get(4)?,
            deleted_at: row.get(5)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn restore_trash_item(conn: &Connection, item_type: &str, item_id: &str) -> Result<()> {
    match item_type {
        "volume" => {
            conn.execute(
                "UPDATE volume SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?1",
                params![item_id],
            )?;
            // Also restore chapters under this volume that were deleted at the same time
            conn.execute(
                "UPDATE chapter SET deleted_at = NULL, updated_at = datetime('now') WHERE volume_id = ?1",
                params![item_id],
            )?;
        }
        "chapter" => {
            conn.execute(
                "UPDATE chapter SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?1",
                params![item_id],
            )?;
            // If the volume of this chapter is currently deleted, restore that volume too
            conn.execute(
                "UPDATE volume SET deleted_at = NULL, updated_at = datetime('now') 
                 WHERE id = (SELECT volume_id FROM chapter WHERE id = ?1)",
                params![item_id],
            )?;
        }
        "worldview" => {
            conn.execute(
                "UPDATE worldview_entry SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?1",
                params![item_id],
            )?;
        }
        "character" => {
            conn.execute(
                "UPDATE character SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?1",
                params![item_id],
            )?;
        }
        "plot" => {
            conn.execute(
                "UPDATE plot_item SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?1",
                params![item_id],
            )?;
        }
        _ => {}
    }
    Ok(())
}

pub fn purge_trash_item(conn: &Connection, item_type: &str, item_id: &str) -> Result<()> {
    match item_type {
        "volume" => {
            conn.execute("DELETE FROM chapter WHERE volume_id = ?1", params![item_id])?;
            conn.execute("DELETE FROM volume WHERE id = ?1", params![item_id])?;
        }
        "chapter" => {
            conn.execute("DELETE FROM chapter WHERE id = ?1", params![item_id])?;
        }
        "worldview" => {
            conn.execute("DELETE FROM worldview_entry WHERE id = ?1", params![item_id])?;
        }
        "character" => {
            conn.execute("DELETE FROM character WHERE id = ?1", params![item_id])?;
        }
        "plot" => {
            conn.execute("DELETE FROM plot_item WHERE id = ?1", params![item_id])?;
        }
        _ => {}
    }
    Ok(())
}

pub fn empty_trash(conn: &Connection, novel_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM chapter WHERE novel_id = ?1 AND deleted_at IS NOT NULL",
        params![novel_id],
    )?;
    conn.execute(
        "DELETE FROM volume WHERE novel_id = ?1 AND deleted_at IS NOT NULL",
        params![novel_id],
    )?;
    conn.execute(
        "DELETE FROM worldview_entry WHERE novel_id = ?1 AND deleted_at IS NOT NULL",
        params![novel_id],
    )?;
    conn.execute(
        "DELETE FROM character WHERE novel_id = ?1 AND deleted_at IS NOT NULL",
        params![novel_id],
    )?;
    conn.execute(
        "DELETE FROM plot_item WHERE novel_id = ?1 AND deleted_at IS NOT NULL",
        params![novel_id],
    )?;
    Ok(())
}

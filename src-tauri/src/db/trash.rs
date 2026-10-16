use rusqlite::{params, Connection, Result};
use shared_types::TrashItem;

/// 列出**已删除的作品**。
///
/// 与 `list_trash_items` 的「以当前作品为作用域」不同：被删除的 novel 不属于
/// 任何现存作品，无法从作品维度查到，因此必须单独提供一个跨作品作用域的入口，
/// 否则「删除作品」就等同于永久丢失。
pub fn list_deleted_novels(conn: &Connection) -> Result<Vec<TrashItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, id AS novel_id, 'novel' AS item_type, title,
                substr(synopsis, 1, 100) AS snippet, deleted_at
         FROM novel
         WHERE deleted_at IS NOT NULL
         ORDER BY deleted_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
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

/// 软删除一个作品，并级联软删除其下全部卷与章节。
///
/// 级联是必要的：`list_volumes` / `list_chapter_headers` 只按 novel_id 过滤，
/// 若只标记 novel，恢复后各卷章的 deleted_at 仍是旧值，会出现「作品回来了但里面空了」
/// 或「卷还留在别的作品的回收站里」这类不一致。
pub fn soft_delete_novel(conn: &Connection, novel_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE novel SET deleted_at = datetime('now') WHERE id = ?1",
        params![novel_id],
    )?;
    conn.execute(
        "UPDATE volume SET deleted_at = datetime('now') WHERE novel_id = ?1 AND deleted_at IS NULL",
        params![novel_id],
    )?;
    conn.execute(
        "UPDATE chapter SET deleted_at = datetime('now') WHERE novel_id = ?1 AND deleted_at IS NULL",
        params![novel_id],
    )?;
    Ok(())
}

/// 恢复一个作品，并把它级联软删除的卷/章一并恢复。
///
/// 只恢复「因这次作品删除而被连带标记」的卷章：用 `soft_delete_novel` 的
/// 同一时间戳条件无法精确区分，因此这里恢复全部同属该作品的卷章——
/// 这与 volume/chapter 的恢复语义一致（恢复卷会连带恢复其章节）。
pub fn restore_novel(conn: &Connection, novel_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE novel SET deleted_at = NULL, updated_at = datetime('now') WHERE id = ?1",
        params![novel_id],
    )?;
    conn.execute(
        "UPDATE volume SET deleted_at = NULL, updated_at = datetime('now') WHERE novel_id = ?1",
        params![novel_id],
    )?;
    conn.execute(
        "UPDATE chapter SET deleted_at = NULL, updated_at = datetime('now') WHERE novel_id = ?1",
        params![novel_id],
    )?;
    Ok(())
}

/// 彻底删除一个作品（连同其全部内容）。外键均为 ON DELETE CASCADE，
/// 故删除 novel 行会连带清理卷/章/设定/对话等所有从属数据。
pub fn purge_novel(conn: &Connection, novel_id: &str) -> Result<()> {
    conn.execute("DELETE FROM novel WHERE id = ?1", params![novel_id])?;
    Ok(())
}

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
        "novel" => {
            restore_novel(conn, item_id)?;
        }
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
        "novel" => {
            purge_novel(conn, item_id)?;
        }
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

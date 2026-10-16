use super::mapper::{query_list, query_optional, NOVEL_COLS};
use rusqlite::{params, Connection, Result};
use shared_types::{Novel, NovelWithStats};
use uuid::Uuid;

fn map_novel(row: &rusqlite::Row<'_>) -> Result<Novel> {
    Ok(Novel {
        id: row.get(0)?,
        title: row.get(1)?,
        synopsis: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        deleted_at: row.get(5)?,
    })
}

pub fn create_novel(conn: &Connection, title: &str, synopsis: &str) -> Result<Novel> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO novel (id, title, synopsis) VALUES (?1, ?2, ?3)",
        params![id, title, synopsis],
    )?;

    get_novel(conn, &id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn list_novels(conn: &Connection) -> Result<Vec<Novel>> {
    query_list(
        conn,
        &format!(
            "SELECT {} FROM novel WHERE deleted_at IS NULL ORDER BY updated_at DESC",
            NOVEL_COLS
        ),
        [],
        map_novel,
    )
}

pub fn list_novels_with_stats(conn: &Connection) -> Result<Vec<NovelWithStats>> {
    let mut stmt = conn.prepare(
        "SELECT 
            n.id, n.title, n.synopsis, n.created_at, n.updated_at,
            (SELECT COUNT(*) FROM volume v WHERE v.novel_id = n.id AND v.deleted_at IS NULL) AS volume_count,
            (SELECT COUNT(*) FROM chapter c WHERE c.novel_id = n.id AND c.deleted_at IS NULL) AS chapter_count,
            (SELECT COALESCE(SUM(c.word_count), 0) FROM chapter c WHERE c.novel_id = n.id AND c.deleted_at IS NULL) AS word_count
         FROM novel n
         WHERE n.deleted_at IS NULL
         ORDER BY n.updated_at DESC"
    )?;
    let iter = stmt.query_map([], |row| {
        Ok(NovelWithStats {
            id: row.get(0)?,
            title: row.get(1)?,
            synopsis: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            volume_count: row.get(5)?,
            chapter_count: row.get(6)?,
            word_count: row.get(7)?,
        })
    })?;
    let mut res = Vec::new();
    for item in iter {
        res.push(item?);
    }
    Ok(res)
}

pub fn get_novel(conn: &Connection, id: &str) -> Result<Option<Novel>> {
    query_optional(
        conn,
        &format!(
            "SELECT {} FROM novel WHERE id = ?1 AND deleted_at IS NULL",
            NOVEL_COLS
        ),
        params![id],
        map_novel,
    )
}

pub fn update_novel(conn: &Connection, id: &str, title: &str, synopsis: &str) -> Result<()> {
    conn.execute(
        "UPDATE novel 
         SET title = ?1, synopsis = ?2, updated_at = datetime('now') 
         WHERE id = ?3",
        params![title, synopsis, id],
    )?;
    Ok(())
}

/// 软删除作品。委托给 `db::trash::soft_delete_novel`，
/// 以确保「删除作品」在任何调用路径下都会连带软删除其卷与章节——
/// 分散实现两份级联逻辑迟早会漏掉一处。
pub fn delete_novel(conn: &Connection, id: &str) -> Result<()> {
    super::trash::soft_delete_novel(conn, id)
}

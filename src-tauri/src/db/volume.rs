use super::mapper::{query_list, query_optional, VOLUME_COLS};
use rusqlite::{params, Connection, Result};
use shared_types::Volume;
use uuid::Uuid;

fn map_volume(row: &rusqlite::Row<'_>) -> Result<Volume> {
    Ok(Volume {
        id: row.get(0)?,
        novel_id: row.get(1)?,
        title: row.get(2)?,
        summary: row.get(3)?,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        deleted_at: row.get(7)?,
    })
}

pub fn create_volume(conn: &Connection, novel_id: &str, title: &str, summary: &str) -> Result<Volume> {
    let id = Uuid::new_v4().to_string();

    // Find max sort_order
    let max_order: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM volume WHERE novel_id = ?1 AND deleted_at IS NULL",
        params![novel_id],
        |row| row.get(0),
    ).unwrap_or(-1);

    conn.execute(
        "INSERT INTO volume (id, novel_id, title, summary, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, novel_id, title, summary, max_order + 1],
    )?;

    get_volume(conn, &id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn list_volumes(conn: &Connection, novel_id: &str) -> Result<Vec<Volume>> {
    query_list(
        conn,
        &format!(
            "SELECT {} FROM volume 
             WHERE novel_id = ?1 AND deleted_at IS NULL 
             ORDER BY sort_order ASC, created_at ASC",
            VOLUME_COLS
        ),
        params![novel_id],
        map_volume,
    )
}

pub fn get_volume(conn: &Connection, id: &str) -> Result<Option<Volume>> {
    query_optional(
        conn,
        &format!(
            "SELECT {} FROM volume WHERE id = ?1 AND deleted_at IS NULL",
            VOLUME_COLS
        ),
        params![id],
        map_volume,
    )
}

pub fn update_volume(conn: &Connection, id: &str, title: &str, summary: &str) -> Result<()> {
    conn.execute(
        "UPDATE volume 
         SET title = ?1, summary = ?2, updated_at = datetime('now') 
         WHERE id = ?3",
        params![title, summary, id],
    )?;
    Ok(())
}

pub fn reorder_volumes(conn: &Connection, volume_ids: &[String]) -> Result<()> {
    for (idx, id) in volume_ids.iter().enumerate() {
        conn.execute(
            "UPDATE volume SET sort_order = ?1 WHERE id = ?2",
            params![idx as i32, id],
        )?;
    }
    Ok(())
}

pub fn delete_volume(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE volume SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    // Cascade soft-delete chapters
    conn.execute(
        "UPDATE chapter SET deleted_at = datetime('now') WHERE volume_id = ?1",
        params![id],
    )?;
    Ok(())
}

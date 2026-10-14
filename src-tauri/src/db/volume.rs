use rusqlite::{params, Connection, Result};
use shared_types::Volume;
use uuid::Uuid;

pub fn create_volume(conn: &Connection, novel_id: &str, title: &str, summary: &str) -> Result<Volume> {
    let id = Uuid::new_v4().to_string();
    
    // Find max sort_order
    let max_order: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) FROM volume WHERE novel_id = ?1 AND deleted_at IS NULL",
        params![novel_id],
        |row| row.get(0),
    ).unwrap_or(-1);

    let next_order = max_order + 1;

    conn.execute(
        "INSERT INTO volume (id, novel_id, title, summary, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, novel_id, title, summary, next_order],
    )?;

    get_volume(conn, &id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn list_volumes(conn: &Connection, novel_id: &str) -> Result<Vec<Volume>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, title, summary, sort_order, created_at, updated_at, deleted_at 
         FROM volume 
         WHERE novel_id = ?1 AND deleted_at IS NULL 
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
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
    })?;

    let mut volumes = Vec::new();
    for r in rows {
        volumes.push(r?);
    }
    Ok(volumes)
}

pub fn get_volume(conn: &Connection, id: &str) -> Result<Option<Volume>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, title, summary, sort_order, created_at, updated_at, deleted_at 
         FROM volume 
         WHERE id = ?1 AND deleted_at IS NULL",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
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
    })?;

    if let Some(res) = rows.next() {
        Ok(Some(res?))
    } else {
        Ok(None)
    }
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

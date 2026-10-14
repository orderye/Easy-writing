use rusqlite::{params, Connection, Result};
use shared_types::Novel;
use uuid::Uuid;

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
    let mut stmt = conn.prepare(
        "SELECT id, title, synopsis, created_at, updated_at, deleted_at 
         FROM novel 
         WHERE deleted_at IS NULL 
         ORDER BY updated_at DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(Novel {
            id: row.get(0)?,
            title: row.get(1)?,
            synopsis: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            deleted_at: row.get(5)?,
        })
    })?;

    let mut novels = Vec::new();
    for r in rows {
        novels.push(r?);
    }
    Ok(novels)
}

pub fn get_novel(conn: &Connection, id: &str) -> Result<Option<Novel>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, synopsis, created_at, updated_at, deleted_at 
         FROM novel 
         WHERE id = ?1 AND deleted_at IS NULL",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        Ok(Novel {
            id: row.get(0)?,
            title: row.get(1)?,
            synopsis: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            deleted_at: row.get(5)?,
        })
    })?;

    if let Some(res) = rows.next() {
        Ok(Some(res?))
    } else {
        Ok(None)
    }
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

pub fn delete_novel(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE novel SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

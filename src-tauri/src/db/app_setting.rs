use rusqlite::{params, Connection, Result};

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM app_setting WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| row.get(0))?;
    if let Some(r) = rows.next() {
        Ok(Some(r?))
    } else {
        Ok(None)
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

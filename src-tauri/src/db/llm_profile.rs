use rusqlite::{params, Connection, Result};
use shared_types::LlmProfile;
use uuid::Uuid;

pub fn create_profile(
    conn: &Connection,
    name: &str,
    protocol: &str,
    base_url: &str,
    key_ref: &str,
    model: &str,
    params_json: &str,
    is_default: bool,
) -> Result<LlmProfile> {
    let id = Uuid::new_v4().to_string();

    if is_default {
        conn.execute("UPDATE llm_profile SET is_default = 0", [])?;
    }

    conn.execute(
        "INSERT INTO llm_profile (id, name, protocol, base_url, key_ref, model, params, is_default)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            name,
            protocol,
            base_url,
            key_ref,
            model,
            params_json,
            if is_default { 1 } else { 0 }
        ],
    )?;

    get_profile(conn, &id).and_then(|opt| {
        opt.ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)
    })
}

pub fn list_profiles(conn: &Connection) -> Result<Vec<LlmProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, protocol, base_url, key_ref, model, params, is_default, created_at, updated_at
         FROM llm_profile
         ORDER BY is_default DESC, created_at ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        let is_default_int: i32 = row.get(7)?;
        Ok(LlmProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            protocol: row.get(2)?,
            base_url: row.get(3)?,
            key_ref: row.get(4)?,
            model: row.get(5)?,
            params: row.get(6)?,
            is_default: is_default_int == 1,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn get_profile(conn: &Connection, id: &str) -> Result<Option<LlmProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, protocol, base_url, key_ref, model, params, is_default, created_at, updated_at
         FROM llm_profile WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        let is_default_int: i32 = row.get(7)?;
        Ok(LlmProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            protocol: row.get(2)?,
            base_url: row.get(3)?,
            key_ref: row.get(4)?,
            model: row.get(5)?,
            params: row.get(6)?,
            is_default: is_default_int == 1,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?;

    if let Some(r) = rows.next() {
        Ok(Some(r?))
    } else {
        Ok(None)
    }
}

pub fn update_profile(
    conn: &Connection,
    id: &str,
    name: &str,
    protocol: &str,
    base_url: &str,
    model: &str,
    params_json: &str,
    is_default: bool,
) -> Result<()> {
    if is_default {
        conn.execute("UPDATE llm_profile SET is_default = 0", [])?;
    }

    conn.execute(
        "UPDATE llm_profile 
         SET name = ?1, protocol = ?2, base_url = ?3, model = ?4, params = ?5, is_default = ?6, updated_at = datetime('now')
         WHERE id = ?7",
        params![
            name,
            protocol,
            base_url,
            model,
            params_json,
            if is_default { 1 } else { 0 },
            id
        ],
    )?;
    Ok(())
}

pub fn delete_profile(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM llm_profile WHERE id = ?1", params![id])?;
    Ok(())
}

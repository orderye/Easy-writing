use rusqlite::{params, Connection, Result};
use shared_types::Agent;

pub fn list_agents(conn: &Connection) -> Result<Vec<Agent>> {
    let mut stmt = conn.prepare(
        "SELECT id, key, name, system_prompt, llm_profile_id, params, enabled, created_at, updated_at
         FROM agent ORDER BY key ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        let enabled_int: i32 = row.get(6)?;
        Ok(Agent {
            id: row.get(0)?,
            key: row.get(1)?,
            name: row.get(2)?,
            system_prompt: row.get(3)?,
            llm_profile_id: row.get(4)?,
            params: row.get(5)?,
            enabled: enabled_int == 1,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn get_agent_by_key(conn: &Connection, key: &str) -> Result<Option<Agent>> {
    let mut stmt = conn.prepare(
        "SELECT id, key, name, system_prompt, llm_profile_id, params, enabled, created_at, updated_at
         FROM agent WHERE key = ?1",
    )?;

    let mut rows = stmt.query_map(params![key], |row| {
        let enabled_int: i32 = row.get(6)?;
        Ok(Agent {
            id: row.get(0)?,
            key: row.get(1)?,
            name: row.get(2)?,
            system_prompt: row.get(3)?,
            llm_profile_id: row.get(4)?,
            params: row.get(5)?,
            enabled: enabled_int == 1,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    })?;

    if let Some(r) = rows.next() {
        Ok(Some(r?))
    } else {
        Ok(None)
    }
}

pub fn update_agent(
    conn: &Connection,
    key: &str,
    system_prompt: &str,
    llm_profile_id: Option<String>,
    params_json: &str,
    enabled: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE agent 
         SET system_prompt = ?1, llm_profile_id = ?2, params = ?3, enabled = ?4, updated_at = datetime('now')
         WHERE key = ?5",
        params![
            system_prompt,
            llm_profile_id,
            params_json,
            if enabled { 1 } else { 0 },
            key
        ],
    )?;
    Ok(())
}

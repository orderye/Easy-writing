use super::mapper::{int_to_bool, query_list, query_optional, AGENT_COLS};
use rusqlite::{params, Connection, Result};
use shared_types::Agent;

fn map_agent(row: &rusqlite::Row<'_>) -> Result<Agent> {
    let enabled_int: i32 = row.get(6)?;
    Ok(Agent {
        id: row.get(0)?,
        key: row.get(1)?,
        name: row.get(2)?,
        system_prompt: row.get(3)?,
        llm_profile_id: row.get(4)?,
        params: row.get(5)?,
        enabled: int_to_bool(enabled_int),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn list_agents(conn: &Connection) -> Result<Vec<Agent>> {
    query_list(
        conn,
        &format!("SELECT {} FROM agent ORDER BY key ASC", AGENT_COLS),
        [],
        map_agent,
    )
}

pub fn get_agent_by_key(conn: &Connection, key: &str) -> Result<Option<Agent>> {
    query_optional(
        conn,
        &format!("SELECT {} FROM agent WHERE key = ?1", AGENT_COLS),
        params![key],
        map_agent,
    )
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

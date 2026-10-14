use rusqlite::{params, Connection, Result};
use shared_types::CharacterRelation;
use uuid::Uuid;

pub fn list_character_relations(conn: &Connection, novel_id: &str) -> Result<Vec<CharacterRelation>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, from_char_id, to_char_id, relation_name, description, created_at, updated_at
         FROM character_relation
         WHERE novel_id = ?1
         ORDER BY created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(CharacterRelation {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            from_char_id: row.get(2)?,
            to_char_id: row.get(3)?,
            relation_name: row.get(4)?,
            description: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_character_relation(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    from_char_id: &str,
    to_char_id: &str,
    relation_name: &str,
    description: &str,
) -> Result<CharacterRelation> {
    let rel_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());

    conn.execute(
        "INSERT INTO character_relation (id, novel_id, from_char_id, to_char_id, relation_name, description, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
            from_char_id = excluded.from_char_id,
            to_char_id = excluded.to_char_id,
            relation_name = excluded.relation_name,
            description = excluded.description,
            updated_at = datetime('now')",
        params![rel_id, novel_id, from_char_id, to_char_id, relation_name, description],
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, from_char_id, to_char_id, relation_name, description, created_at, updated_at
         FROM character_relation WHERE id = ?1",
    )?;

    stmt.query_row(params![rel_id], |row| {
        Ok(CharacterRelation {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            from_char_id: row.get(2)?,
            to_char_id: row.get(3)?,
            relation_name: row.get(4)?,
            description: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })
}

pub fn delete_character_relation(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM character_relation WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

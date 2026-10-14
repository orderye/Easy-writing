use rusqlite::{params, Connection, Result};
use shared_types::{Character, PlotItem, WorldviewEntry};
use uuid::Uuid;

// --- Worldview ---
pub fn list_worldview(conn: &Connection, novel_id: &str) -> Result<Vec<WorldviewEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, group_name, title, content, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
         FROM worldview_entry
         WHERE novel_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(WorldviewEntry {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            group_name: row.get(2)?,
            title: row.get(3)?,
            content: row.get(4)?,
            origin: row.get(5)?,
            source_chapter_id: row.get(6)?,
            sort_order: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            deleted_at: row.get(10)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_worldview(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    group_name: &str,
    title: &str,
    content: &str,
) -> Result<WorldviewEntry> {
    let entry_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO worldview_entry (id, novel_id, group_name, title, content, origin, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, 'manual', 0)
         ON CONFLICT(id) DO UPDATE SET
            group_name = excluded.group_name,
            title = excluded.title,
            content = excluded.content,
            updated_at = datetime('now')",
        params![entry_id, novel_id, group_name, title, content],
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, group_name, title, content, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
         FROM worldview_entry WHERE id = ?1",
    )?;
    stmt.query_row(params![entry_id], |row| {
        Ok(WorldviewEntry {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            group_name: row.get(2)?,
            title: row.get(3)?,
            content: row.get(4)?,
            origin: row.get(5)?,
            source_chapter_id: row.get(6)?,
            sort_order: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            deleted_at: row.get(10)?,
        })
    })
}

pub fn delete_worldview(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE worldview_entry SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Character ---
pub fn list_characters(conn: &Connection, novel_id: &str) -> Result<Vec<Character>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, name, aliases, profile, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
         FROM character
         WHERE novel_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(Character {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            name: row.get(2)?,
            aliases: row.get(3)?,
            profile: row.get(4)?,
            origin: row.get(5)?,
            source_chapter_id: row.get(6)?,
            sort_order: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            deleted_at: row.get(10)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_character(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    name: &str,
    aliases: &str,
    profile: &str,
) -> Result<Character> {
    let char_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO character (id, novel_id, name, aliases, profile, origin, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, 'manual', 0)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            aliases = excluded.aliases,
            profile = excluded.profile,
            updated_at = datetime('now')",
        params![char_id, novel_id, name, aliases, profile],
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, name, aliases, profile, origin, source_chapter_id, sort_order, created_at, updated_at, deleted_at
         FROM character WHERE id = ?1",
    )?;
    stmt.query_row(params![char_id], |row| {
        Ok(Character {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            name: row.get(2)?,
            aliases: row.get(3)?,
            profile: row.get(4)?,
            origin: row.get(5)?,
            source_chapter_id: row.get(6)?,
            sort_order: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            deleted_at: row.get(10)?,
        })
    })
}

pub fn delete_character(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE character SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Plot ---
pub fn list_plots(conn: &Connection, novel_id: &str) -> Result<Vec<PlotItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, title, content, kind, status, chapter_id, origin, sort_order, created_at, updated_at, deleted_at
         FROM plot_item
         WHERE novel_id = ?1 AND deleted_at IS NULL
         ORDER BY sort_order ASC, created_at ASC",
    )?;

    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(PlotItem {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            kind: row.get(4)?,
            status: row.get(5)?,
            chapter_id: row.get(6)?,
            origin: row.get(7)?,
            sort_order: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            deleted_at: row.get(11)?,
        })
    })?;

    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn save_plot(
    conn: &Connection,
    id: Option<String>,
    novel_id: &str,
    title: &str,
    content: &str,
    kind: &str,
    status: &str,
    chapter_id: Option<String>,
) -> Result<PlotItem> {
    let plot_id = id.unwrap_or_else(|| Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO plot_item (id, novel_id, title, content, kind, status, chapter_id, origin, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual', 0)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            kind = excluded.kind,
            status = excluded.status,
            chapter_id = excluded.chapter_id,
            updated_at = datetime('now')",
        params![plot_id, novel_id, title, content, kind, status, chapter_id],
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, novel_id, title, content, kind, status, chapter_id, origin, sort_order, created_at, updated_at, deleted_at
         FROM plot_item WHERE id = ?1",
    )?;
    stmt.query_row(params![plot_id], |row| {
        Ok(PlotItem {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            kind: row.get(4)?,
            status: row.get(5)?,
            chapter_id: row.get(6)?,
            origin: row.get(7)?,
            sort_order: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
            deleted_at: row.get(11)?,
        })
    })
}

pub fn delete_plot(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE plot_item SET deleted_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

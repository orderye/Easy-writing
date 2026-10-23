//! 「本章忽略此类提示」的持久化（T08 / V007）。
//!
//! 签名由 `ai::consistency::resolve_related_settings` 统一计算：
//! `<entity_type>:<id>`（能定位到具体设定）或 `module:<类型>`（只能归类）。
//! 前端只回传后端给的签名，避免两端各算一套。
use rusqlite::{params, Connection, Result};
use std::collections::HashSet;
use uuid::Uuid;

/// 读取某章已忽略的签名集合。
///
/// 查询失败必须冒泡：把失败当成「没有忽略记录」会让作者刚点过的
/// 「别再提示」失效，而他不会知道。
pub fn list_signatures(conn: &Connection, chapter_id: &str) -> Result<HashSet<String>> {
    let mut stmt =
        conn.prepare("SELECT signature FROM chapter_conflict_ignore WHERE chapter_id = ?1")?;
    let rows = stmt.query_map(params![chapter_id], |row| row.get::<_, String>(0))?;
    let mut set = HashSet::new();
    for r in rows {
        set.insert(r?);
    }
    Ok(set)
}

/// 记录「本章忽略此类提示」。重复点击同一签名只更新展示文案，不报错。
pub fn ignore(
    conn: &Connection,
    novel_id: &str,
    chapter_id: &str,
    signature: &str,
    label: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO chapter_conflict_ignore (id, novel_id, chapter_id, signature, label)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(novel_id, chapter_id, signature)
         DO UPDATE SET label = excluded.label",
        params![
            Uuid::new_v4().to_string(),
            novel_id,
            chapter_id,
            signature,
            label
        ],
    )?;
    Ok(())
}

/// 清除某章的全部忽略记录。
///
/// 目前没有 UI 入口（「本章忽略」按章持久是设计意图）；保留它是为了测试，
/// 以及将来若加「恢复本章提示」按钮时不必再动数据层。
pub fn clear_chapter(conn: &Connection, chapter_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM chapter_conflict_ignore WHERE chapter_id = ?1",
        params![chapter_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_with_table() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../migrations/V007__chapter_conflict_ignore.sql"))
            .unwrap();
        conn
    }

    #[test]
    fn ignore_is_idempotent_and_readable() {
        let conn = conn_with_table();
        ignore(&conn, "n1", "c1", "character:ch1", "角色·沈砚").unwrap();
        ignore(&conn, "n1", "c1", "character:ch1", "角色·沈砚（改）").unwrap();

        let set = list_signatures(&conn, "c1").unwrap();
        assert_eq!(set.len(), 1, "同一签名不应产生两行");
        assert!(set.contains("character:ch1"));

        let label: String = conn
            .query_row(
                "SELECT label FROM chapter_conflict_ignore WHERE chapter_id = 'c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(label, "角色·沈砚（改）");
    }

    #[test]
    fn ignores_are_scoped_to_chapter() {
        let conn = conn_with_table();
        ignore(&conn, "n1", "c1", "character:ch1", "角色·沈砚").unwrap();
        assert!(list_signatures(&conn, "c2").unwrap().is_empty());

        clear_chapter(&conn, "c1").unwrap();
        assert!(list_signatures(&conn, "c1").unwrap().is_empty());
    }
}
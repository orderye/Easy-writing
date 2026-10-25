//! 每日字数统计（任务书 T11）。
//!
//! 口径：净增字数在 `save_chapter_content` 里按「本次保存的 word_count −
//! 上一次保存的 word_count」累加，允许负值（删稿如实扣减）。
//! 导入（`commit_import_novel` 直接 INSERT）与批量替换不经过该命令，
//! 天然不计入 —— 这正是任务书验收要求的「导入整书不会产生巨额今日字数」。

use rusqlite::{params, Connection, Result};
use shared_types::DailyStat;

const SETTING_KEY: &str = "daily_goal";

/// 当天日期（本地时区，UTC+8 的作者看到的就是 8 日当天）。
///
/// SQLite 的 `date('now','localtime')` 与应用其余处的 datetime 口径一致，
/// 不在 Rust 侧再引一个时间库来拼日期。
fn today(conn: &Connection) -> Result<String> {
    conn.query_row("SELECT date('now', 'localtime')", [], |row| row.get(0))
}

/// 累加某作品今天的净增字数（可为负）。行不存在则创建。
///
/// 失败不上抛给调用方：统计是附属功能，绝不能因为统计行写不进去
/// 而让「保存正文」整体失败。
pub fn add_net_words(conn: &Connection, novel_id: &str, delta: i64) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let date = today(conn)?;
    conn.execute(
        "INSERT INTO daily_stat (id, novel_id, stat_date, net_words)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(novel_id, stat_date) DO UPDATE SET
             net_words = net_words + excluded.net_words,
             updated_at = datetime('now')",
        params![uuid::Uuid::new_v4().to_string(), novel_id, date, delta],
    )?;
    Ok(())
}

/// 查某作品某天的净增字数；无记录返回 0。
pub fn get_net_words(conn: &Connection, novel_id: &str, stat_date: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(net_words), 0) FROM daily_stat
         WHERE novel_id = ?1 AND stat_date = ?2",
        params![novel_id, stat_date],
        |row| row.get(0),
    )
}

/// 查某作品今天的净增字数（状态栏用）。
pub fn get_today(conn: &Connection, novel_id: &str) -> Result<DailyStat> {
    let date = today(conn)?;
    Ok(DailyStat {
        stat_date: date.clone(),
        net_words: get_net_words(conn, novel_id, &date)?,
    })
}

/// 每日目标字数（存 app_setting）。未设置返回 0 = 不显示目标。
pub fn get_goal(conn: &Connection) -> Result<i64> {
    let raw = super::app_setting::get_setting(conn, SETTING_KEY)?
        .unwrap_or_default();
    Ok(raw.parse().unwrap_or(0))
}

pub fn set_goal(conn: &Connection, words: i64) -> Result<()> {
    // 负数目标毫无意义，夹到 0（0 = 不设目标）。
    let clamped = words.max(0);
    super::app_setting::set_setting(conn, SETTING_KEY, &clamped.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("../../migrations/V001__init.sql"))
            .unwrap();
        conn.execute_batch(include_str!(
            "../../migrations/V009__daily_stat_and_chapter_pov.sql"
        ))
        .unwrap();
        conn
    }

    #[test]
    fn upsert_accumulates_delta() {
        let conn = setup();
        conn.execute("INSERT INTO novel (id) VALUES ('n1')", []).unwrap();

        add_net_words(&conn, "n1", 500).unwrap();
        add_net_words(&conn, "n1", 300).unwrap();
        let stat = get_today(&conn, "n1").unwrap();
        assert_eq!(stat.net_words, 800);
        // 同一天同作品只有一行
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stat", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn negative_delta_is_kept() {
        let conn = setup();
        conn.execute("INSERT INTO novel (id) VALUES ('n1')", []).unwrap();

        add_net_words(&conn, "n1", 500).unwrap();
        add_net_words(&conn, "n1", -1200).unwrap();
        assert_eq!(get_today(&conn, "n1").unwrap().net_words, -700);
    }

    #[test]
    fn zero_delta_writes_nothing() {
        let conn = setup();
        conn.execute("INSERT INTO novel (id) VALUES ('n1')", []).unwrap();

        add_net_words(&conn, "n1", 0).unwrap();
        assert_eq!(get_net_words(&conn, "n1", "2099-01-01").unwrap(), 0);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stat", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn other_novel_is_not_counted() {
        let conn = setup();
        conn.execute("INSERT INTO novel (id) VALUES ('n1')", []).unwrap();
        conn.execute("INSERT INTO novel (id) VALUES ('n2')", []).unwrap();

        add_net_words(&conn, "n1", 500).unwrap();
        assert_eq!(get_today(&conn, "n2").unwrap().net_words, 0);
    }

    #[test]
    fn deleting_novel_cascades_stat() {
        let conn = setup();
        conn.execute("INSERT INTO novel (id) VALUES ('n1')", []).unwrap();
        add_net_words(&conn, "n1", 500).unwrap();
        conn.execute("DELETE FROM novel WHERE id = 'n1'", []).unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stat", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn goal_round_trips_and_clamps() {
        let conn = setup();
        assert_eq!(get_goal(&conn).unwrap(), 0);
        set_goal(&conn, 2000).unwrap();
        assert_eq!(get_goal(&conn).unwrap(), 2000);
        set_goal(&conn, -5).unwrap();
        assert_eq!(get_goal(&conn).unwrap(), 0);
    }
}

//! 迁移阶梯（schema_version）的回归测试。
//!
//! 守三件事：
//!   1. 新库能正常建起来，并记录到基线版本；
//!   2. 重复初始化不报错、版本号不增长（幂等）；
//!   3. 缺列的「老库」能被补齐，而不是静默失败。
//!
//! 第 3 条尤其重要：旧实现用 let _ = 吞掉所有 ALTER 错误，
//! 磁盘只读 / 库被锁时用户完全看不到问题，只是某天发现数据对不上。
use easy_writing_lib::db;
use rusqlite::Connection;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ew_mig_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    names.iter().any(|n| n == column)
}

fn schema_version(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn fresh_database_reaches_baseline_and_keeps_data() {
    let dir = temp_dir("fresh");

    let novel_id = {
        let pool = db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(schema_version(&conn), 2, "新库应记录到基线版本");
        assert!(column_exists(&conn, "novel_lexicon", "term"));
        assert!(column_exists(&conn, "character_relation", "relation_name"));
        db::novel::create_novel(&conn, "迁移测试", "").unwrap().id
    };

    // 再初始化一次：数据必须还在，版本号不变
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    assert_eq!(schema_version(&conn), 2);
    assert_eq!(
        db::novel::get_novel(&conn, &novel_id).unwrap().unwrap().title,
        "迁移测试"
    );
}

#[test]
fn repeated_init_is_idempotent() {
    let dir = temp_dir("repeat");
    for i in 0..3 {
        let pool = db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(schema_version(&conn), 2, "第 {} 次初始化后版本号不应增长", i + 1);
    }
}

#[test]
fn legacy_database_gets_missing_columns_backfilled() {
    let dir = temp_dir("legacy");
    let db_path = dir.join("easy-writing.db");

    // 手工造一个「老库」：chat_session 缺 title、asset 缺 deleted_at、
    // setting_change 缺 target_title / is_reverted。
    // 注意不能靠「只跑 V001」来造老库 —— V001 早已被更新成含这些列，
    // 所以必须显式建出缺列的表。
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_session (
                 id TEXT PRIMARY KEY,
                 novel_id TEXT NOT NULL,
                 chapter_id TEXT,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE asset (
                 id TEXT PRIMARY KEY,
                 novel_id TEXT NOT NULL,
                 name TEXT NOT NULL,
                 mime TEXT NOT NULL,
                 rel_path TEXT NOT NULL,
                 size INTEGER NOT NULL DEFAULT 0,
                 hash TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE setting_change (
                 id TEXT PRIMARY KEY,
                 novel_id TEXT NOT NULL,
                 target_type TEXT NOT NULL,
                 target_id TEXT NOT NULL,
                 before_content TEXT NOT NULL DEFAULT '',
                 after_content TEXT NOT NULL DEFAULT '',
                 reason TEXT NOT NULL DEFAULT '',
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .unwrap();

        // 前置断言：确认「老库」确实缺列，否则这个用例会变成空跑
        assert!(!column_exists(&conn, "chat_session", "title"));
        assert!(!column_exists(&conn, "asset", "deleted_at"));
        assert!(!column_exists(&conn, "setting_change", "target_title"));
        assert!(!column_exists(&conn, "setting_change", "is_reverted"));
    }

    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), 2);
    assert!(column_exists(&conn, "chat_session", "title"), "老库缺的 title 必须补上");
    assert!(column_exists(&conn, "asset", "deleted_at"));
    assert!(column_exists(&conn, "setting_change", "target_title"));
    assert!(column_exists(&conn, "setting_change", "is_reverted"));
}

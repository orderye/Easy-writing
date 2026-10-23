use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

pub mod agent;
pub mod app_setting;
pub mod chapter;
pub mod chapter_revision;
pub mod chat;
pub mod character_relation;
pub mod extraction;
pub mod lexicon;
pub mod llm_profile;
pub mod mapper;
pub mod novel;
pub mod prompt_template;
pub mod search;
pub mod settings;
pub mod trash;
pub mod volume;

pub type DbPool = Pool<SqliteConnectionManager>;

/// 迁移进度表。
const SCHEMA_VERSION_DDL: &str = "CREATE TABLE IF NOT EXISTS schema_version (
    version     INTEGER PRIMARY KEY,
    applied_at  TEXT NOT NULL DEFAULT (datetime('now'))
);";

/// 引入 schema_version 之前，那些「每次启动无条件执行」的安全迁移。
///
/// 老库上它们早就生效过，重复执行只会报 duplicate column —— 这正是下面
/// run_legacy_safe_migrations 需要显式容忍的错误，而不是像旧实现那样一律吞掉。
const LEGACY_SAFE_MIGRATIONS: &str =
    include_str!("../../migrations/V002__legacy_safe_migrations.sql");

/// 基线版本：老库在没有任何 schema_version 记录时，视为已经跑到这里。
pub const BASELINE_VERSION: i64 = 6;

/// 只有「列已存在」是预期内的错误；磁盘只读、库被锁、语法错误都必须冒泡。
fn is_duplicate_column(e: &rusqlite::Error) -> bool {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => msg.contains("duplicate column name"),
        _ => false,
    }
}

/// 逐条执行 V002。
///
/// V002 的语句保证「一行一条、以分号结尾，且字符串字面量里不含分号」，
/// 因此按 ';' 朴素切分是安全的（见该文件顶部注释的约束说明）。
fn run_legacy_safe_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    // 先剥掉整行注释再切分。
    //
    // 这个顺序是被真实故障逼出来的：V002 的注释里出现过 "按 ';' 切分" 这样的
    // 说明文字，直接用 split(';') 会把注释从中间切开，拼出一条以引号开头的
    // 伪语句交给 SQLite，报 unrecognized token。
    let sql = strip_line_comments(LEGACY_SAFE_MIGRATIONS);
    for stmt in sql.split(';') {
        let stmt = stmt.trim();
        // 纯注释/空白片段必须跳过：把它交给 prepare 会得到一个空语句，
        // rusqlite 会报 SQLITE_MISUSE + "not an error" —— 这个坑是被本函数的
        // 测试真实踩出来的，别删这个判断。
        if is_effectively_empty(stmt) {
            continue;
        }
        match conn.execute(stmt, []) {
            Ok(_) => {}
            Err(e) if is_duplicate_column(&e) => {
                // 老库的正常情况：该列早就被无条件 ALTER 加上过。
            }
            Err(e) => {
                // 迁移失败必须能定位到具体是哪一条，否则用户只拿到一个
                // SQLite 错误码，无从下手。
                eprintln!("[migration] 执行失败：{stmt}\n  错误：{e}");
                return Err(e);
            }
        }
    }
    Ok(())
}

/// 删除所有以 `--` 开头的整行注释。
///
/// 只处理「整行」注释：本迁移文件不使用行尾注释，也不含带 `--` 的字符串字面量。
/// 这条约束一旦被打破，应当改为调用方逐条显式执行，而不是继续加强这个解析器。
fn strip_line_comments(sql: &str) -> String {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 片段是否只有注释和空白（没有可执行的 SQL）。
fn is_effectively_empty(sql: &str) -> bool {
    sql.lines()
        .map(str::trim)
        .all(|line| line.is_empty() || line.starts_with("--"))
}

#[derive(Debug)]
struct PragmaCustomizer;

impl r2d2::CustomizeConnection<Connection, rusqlite::Error> for PragmaCustomizer {
    fn on_acquire(&self, conn: &mut Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(())
    }
}

pub fn init_db(app_data_dir: PathBuf) -> Result<DbPool, Box<dyn std::error::Error>> {
    fs::create_dir_all(&app_data_dir)?;
    
    // Also ensure images directory exists
    let images_dir = app_data_dir.join("images");
    fs::create_dir_all(&images_dir)?;

    let db_path = app_data_dir.join("easy-writing.db");
    let manager = SqliteConnectionManager::file(db_path);
    
    let pool = Pool::builder()
        .max_size(8)
        .connection_customizer(Box::new(PragmaCustomizer))
        .build(manager)?;

    // ---- 迁移阶梯 ----
    //
    // 进度记在 schema_version 表里；每次启动只跑「尚未应用」的部分。
    // 取代了以前「每次启动全量重跑 V001 + 一串 let _ = 吞错误的 ALTER」的做法：
    // 那种写法无法区分「已是最新」与「加列失败」，磁盘只读或库被锁时全部静默。
    let conn = pool.get()?;
    conn.execute_batch(SCHEMA_VERSION_DDL)?;

    let applied: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    if applied < 1 {
        conn.execute_batch(include_str!("../../migrations/V001__init.sql"))?;
        // INSERT OR IGNORE：两个实例同时首启时会并发跑到这里，
        // 普通 INSERT 会因主键冲突直接让启动失败。
        conn.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (1)", [])?;
    }

    if applied < 2 {
        run_legacy_safe_migrations(&conn)?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (2)",
            [],
        )?;
    }

    if applied < 3 {
        conn.execute_batch(include_str!("../../migrations/V003__indices_and_cleanup.sql"))?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (3)",
            [],
        )?;
    }

    if applied < 4 {
        conn.execute_batch(include_str!("../../migrations/V004__entry_pinned.sql"))?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (4)",
            [],
        )?;
    }

    if applied < 5 {
        conn.execute_batch(include_str!("../../migrations/V005__chapter_fts.sql"))?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (5)",
            [],
        )?;
    }

    if applied < 6 {
        conn.execute_batch(include_str!("../../migrations/V006__extraction_duplicate.sql"))?;
        conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (6)",
            [],
        )?;
    }

    Ok(pool)
}

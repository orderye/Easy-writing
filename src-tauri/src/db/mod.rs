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
pub mod settings;
pub mod trash;
pub mod volume;

pub type DbPool = Pool<SqliteConnectionManager>;

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

    // Run initial migration
    let conn = pool.get()?;
    let migration_sql = include_str!("../../migrations/V001__init.sql");
    conn.execute_batch(migration_sql)?;

    // Safe column & table migrations for existing databases
    let _ = conn.execute("ALTER TABLE setting_change ADD COLUMN target_title TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE setting_change ADD COLUMN is_reverted INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS character_relation (
            id              TEXT PRIMARY KEY,
            novel_id        TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
            from_char_id    TEXT NOT NULL REFERENCES character(id) ON DELETE CASCADE,
            to_char_id      TEXT NOT NULL REFERENCES character(id) ON DELETE CASCADE,
            relation_name   TEXT NOT NULL DEFAULT '',
            description     TEXT NOT NULL DEFAULT '',
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_relation_novel ON character_relation(novel_id);",
    );
    let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_prompt_template_scope ON prompt_template(scope)", []);

    // 对话记录（PRD §4.1 P1）：为旧库补齐 chat_session.title 列与索引
    let _ = conn.execute(
        "ALTER TABLE chat_session ADD COLUMN title TEXT NOT NULL DEFAULT '对话'",
        [],
    );
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_chat_session_novel ON chat_session(novel_id, chapter_id, updated_at DESC)",
        [],
    );
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS novel_lexicon (
            id          TEXT PRIMARY KEY,
            novel_id    TEXT NOT NULL REFERENCES novel(id) ON DELETE CASCADE,
            term        TEXT NOT NULL,
            category    TEXT NOT NULL DEFAULT 'custom',
            frequency   INTEGER NOT NULL DEFAULT 1,
            source_id   TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_lexicon_novel_term ON novel_lexicon(novel_id, term);
        CREATE INDEX IF NOT EXISTS idx_lexicon_novel ON novel_lexicon(novel_id);",
    );

    // 图库（素材管理）：为旧库补齐 asset.deleted_at。
    // 与作品/卷/章一致，图片移除走软删除；列可空，失败忽略（新库已由 V001 建好）。
    let _ = conn.execute("ALTER TABLE asset ADD COLUMN deleted_at TEXT", []);

    Ok(pool)
}

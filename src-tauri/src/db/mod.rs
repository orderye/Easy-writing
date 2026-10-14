use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

pub mod agent;
pub mod app_setting;
pub mod chapter;
pub mod chapter_revision;
pub mod extraction;
pub mod llm_profile;
pub mod novel;
pub mod settings;
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

    // Safe column migrations for existing databases
    let _ = conn.execute("ALTER TABLE setting_change ADD COLUMN target_title TEXT NOT NULL DEFAULT ''", []);
    let _ = conn.execute("ALTER TABLE setting_change ADD COLUMN is_reverted INTEGER NOT NULL DEFAULT 0", []);

    Ok(pool)
}

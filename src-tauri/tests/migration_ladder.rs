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
        assert_eq!(schema_version(&conn), db::BASELINE_VERSION, "新库应记录到基线版本");
        assert!(column_exists(&conn, "novel_lexicon", "term"));
        assert!(column_exists(&conn, "character_relation", "relation_name"));
        db::novel::create_novel(&conn, "迁移测试", "").unwrap().id
    };

    // 再初始化一次：数据必须还在，版本号不变
    let pool = db::init_db(dir).unwrap();
    let conn = pool.get().unwrap();
    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
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
        assert_eq!(schema_version(&conn), db::BASELINE_VERSION, "第 {} 次初始化后版本号不应增长", i + 1);
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

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert!(column_exists(&conn, "chat_session", "title"), "老库缺的 title 必须补上");
    assert!(column_exists(&conn, "asset", "deleted_at"));
    assert!(column_exists(&conn, "setting_change", "target_title"));
    assert!(column_exists(&conn, "setting_change", "is_reverted"));
}

// ===========================================================================
// P1-2 · 真实历史 schema 的连续升级
//
// ## 先确认真实的迁移版本（任务书 A 要求）
//
// 从代码与 git 历史核实到的事实：
//
// * `db::BASELINE_VERSION = 4`；迁移文件为 V001__init / V002__legacy_safe_migrations /
//   V003__indices_and_cleanup / V004__entry_pinned；入口是 `db::init_db`。
// * 版本化阶梯是 f88a4e3（v0.3.4 之前）引入的，当时 `BASELINE_VERSION = 2`；
//   V003 与 `BASELINE_VERSION = 3` 是 5e4cf00（v0.3.5）才加入的，
//   V004 与 `BASELINE_VERSION = 4` 是随 T06（上下文装配透明度）加入的。
// * 因此用户真实可能持有的库只有这几种状态：
//   - **无 `schema_version` 表**（≤ v0.3.3，阶梯引入之前的库）→ applied = 0
//   - **{1}**（V001 跑完、V002 之前进程被中断）
//   - **{1,2}**（v0.3.4 产物，缺 V003 的索引）
//   - **{1,2,3}**（v0.3.5–v0.3.8 产物，缺 V004 的 pinned 列）
//   - **{1,2,3,4}**（当前）
//   - **更高版本**（未来客户端写过、被旧客户端打开）
//
// ## fixture 怎么造，以及为什么这样造是诚实的
//
// V001 在阶梯引入之后仍被改过（95d7e5f 给 `extraction_candidate` 加了 `title`），
// 所以「今天跑 V001 得到的库」并不等于「v0.3.4 用户手里的库」。
// 好在那次改动同时在 V002 里补了对应的 `ALTER TABLE ... ADD COLUMN`，
// 即历史库的缺列由 V002 负责回填 —— 这条路径已由上面的
// `legacy_database_gets_missing_columns_backfilled` 覆盖。
//
// 下面的 fixture 采用另一种构造方式：先用当前代码建出完整 v4 库并灌入业务数据，
// 再**按历史状态精确退化**（删掉后续版本才有的表/索引/列，回退 `schema_version`）。
// 这样表结构直接来自当前代码，不会因为 fixture 写错列名而测到一个
// 现实中根本不存在的库；退化掉的东西也逐一列出，可核对。
// ===========================================================================

/// V003 创建的全部索引。v2 及更早状态的库不可能有它们。
const V003_INDEXES: &[&str] = &[
    "idx_asset_novel_hash",
    "idx_asset_hash",
    "idx_plot_item_chapter",
    "idx_setting_change_target",
    "idx_chapter_revision_auto",
];

/// 只由 V002 创建、V001 中没有的对象。v1 状态的库不可能有它们。
const V002_ONLY_OBJECTS: &[&str] = &[
    "idx_lexicon_novel_term",
    "idx_lexicon_novel",
    "idx_prompt_template_scope",
    "novel_lexicon",
];

/// V004 为三张设定表添加的 pinned 列。比 V004 旧的库没有它们，
/// 退化 fixture 时必须物理删掉，表结构才等于真实的历史库
/// （否则重新 init 跑 V004 会撞上「duplicate column name: pinned」）。
fn drop_v004_columns(conn: &Connection) {
    for table in ["character", "worldview_entry", "plot_item"] {
        conn.execute(&format!("ALTER TABLE {table} DROP COLUMN pinned"), [])
            .unwrap();
    }
}

/// V005 创建的对象：FTS5 虚拟表 + 三条触发器 + `batch_id` 列。
///
/// 比 V005 旧的库没有它们；退化 fixture 时必须物理删掉，
/// 否则重新 init 跑 V005 会撞上「table chapter_fts already exists」
/// 或「duplicate column name: batch_id」。
fn drop_v005_objects(conn: &Connection) {
    for trigger in ["chapter_fts_ai", "chapter_fts_au", "chapter_fts_ad"] {
        conn.execute(&format!("DROP TRIGGER IF EXISTS {trigger}"), []).unwrap();
    }
    conn.execute("DROP TABLE IF EXISTS chapter_fts", []).unwrap();
    // 先删索引再删列：SQLite 拒绝删除仍被索引引用的列。
    conn.execute("DROP INDEX IF EXISTS idx_revision_batch", []).unwrap();
    conn.execute("ALTER TABLE chapter_revision DROP COLUMN batch_id", []).unwrap();
}

/// 断言 v5 之前的库没有 FTS 索引与 batch_id 列。
fn assert_v005_objects_absent(conn: &Connection) {
    assert!(!table_exists(conn, "chapter_fts"), "历史状态不应有 V005 的 chapter_fts");
    assert!(
        !column_exists(conn, "chapter_revision", "batch_id"),
        "历史状态不应有 V005 的 batch_id 列"
    );
}

/// 断言 V005 的产物齐全，且存量正文已被回填。
fn assert_v005_objects_present(conn: &Connection) {
    assert!(table_exists(conn, "chapter_fts"), "V005 必须创建 chapter_fts");
    assert!(
        column_exists(conn, "chapter_revision", "batch_id"),
        "V005 必须给 chapter_revision 加 batch_id"
    );
    for trigger in ["chapter_fts_ai", "chapter_fts_au", "chapter_fts_ad"] {
        assert!(
            trigger_exists(conn, trigger),
            "V005 必须创建触发器 {trigger}"
        );
    }
    // 回填：索引行数必须等于未删章节数
    let chapters: i64 = conn
        .query_row("SELECT COUNT(*) FROM chapter WHERE deleted_at IS NULL", [], |r| r.get(0))
        .unwrap();
    let indexed: i64 = conn
        .query_row("SELECT COUNT(*) FROM chapter_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(indexed, chapters, "V005 必须回填全部未删章节（{indexed} vs {chapters}）");
}

fn trigger_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

/// V006 创建的对象：`extraction_candidate.possible_duplicate_of` 列与索引。
fn drop_v006_objects(conn: &Connection) {
    conn.execute("DROP INDEX IF EXISTS idx_extraction_duplicate", []).unwrap();
    conn.execute(
        "ALTER TABLE extraction_candidate DROP COLUMN possible_duplicate_of",
        [],
    )
    .unwrap();
}

fn assert_v006_objects_absent(conn: &Connection) {
    assert!(
        !column_exists(conn, "extraction_candidate", "possible_duplicate_of"),
        "历史状态不应有 V006 的 possible_duplicate_of 列"
    );
}

fn assert_v006_objects_present(conn: &Connection) {
    assert!(
        column_exists(conn, "extraction_candidate", "possible_duplicate_of"),
        "V006 必须给 extraction_candidate 加 possible_duplicate_of"
    );
    assert!(
        index_exists(conn, "idx_extraction_duplicate"),
        "V006 必须创建 idx_extraction_duplicate"
    );
}

/// V007 创建的对象：`chapter_conflict_ignore` 表与其索引。
fn drop_v007_objects(conn: &Connection) {
    conn.execute("DROP TABLE IF EXISTS chapter_conflict_ignore", [])
        .unwrap();
}

fn assert_v007_objects_absent(conn: &Connection) {
    assert!(
        !table_exists(conn, "chapter_conflict_ignore"),
        "历史状态不应有 V007 的 chapter_conflict_ignore"
    );
}

fn assert_v007_objects_present(conn: &Connection) {
    assert!(
        table_exists(conn, "chapter_conflict_ignore"),
        "V007 必须创建 chapter_conflict_ignore"
    );
    assert!(
        index_exists(conn, "idx_conflict_ignore_chapter"),
        "V007 必须创建 idx_conflict_ignore_chapter"
    );
    assert_eq!(
        count(conn, "chapter_conflict_ignore"),
        0,
        "V007 只建表，不得凭空造出忽略记录"
    );
}

/// 目标历史状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoricalState {
    /// ≤ v0.3.3：连 `schema_version` 表都没有。
    NoVersionTable,
    /// V001 已应用，V002/V003 未应用。
    V1,
    /// v0.3.4 产物：V001+V002 已应用，V003 未应用。
    V2,
    /// v0.3.5–v0.3.8 产物：V001–V003 已应用，V004 未应用。
    V3,
    /// 引入 T06 之后、T09 之前的产物：V001–V004 已应用，V005 未应用。
    V4,
    /// 引入 T09 之后、T10 之前的产物：V001–V005 已应用，V006 未应用。
    V5,
    /// 引入 PR-10（T07）之后、PR-11（T08）之前的产物：V001–V006 已应用，V007 未应用。
    V6,
}

fn index_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn versions(conn: &Connection) -> Vec<i64> {
    let mut stmt = conn
        .prepare("SELECT version FROM schema_version ORDER BY version")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, i64>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect()
}

fn applied_at(conn: &Connection, version: i64) -> Option<String> {
    conn.query_row(
        "SELECT applied_at FROM schema_version WHERE version = ?1",
        [version],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// 断言三张设定表都没有 pinned 列（V004 产物）。
fn assert_pinned_columns_absent(conn: &Connection) {
    for table in ["character", "worldview_entry", "plot_item"] {
        assert!(
            !column_exists(&conn, table, "pinned"),
            "历史状态不应有 V004 才创建的 pinned 列（{table}）"
        );
    }
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 迁移前后都必须原样存在的业务数据。正文用可识别的长句，
/// 这样「数据还在」与「数据被截断/清空」能被区分开。
const NOVEL_TITLE: &str = "迁移验证作品·长河";
const CHAPTER_BODY: &str = "第一章正文：夜色顺着河面铺开，灯塔的光每隔七秒扫过一次礁石。";
const CHAPTER_HTML: &str = "<p>第一章正文：夜色顺着河面铺开，灯塔的光每隔七秒扫过一次礁石。</p>";
const WORLDVIEW_TITLE: &str = "世界观·灯塔守望者";
const CHAR_A: &str = "角色甲·沈砚";
const CHAR_B: &str = "角色乙·阿岚";
const RELATION_NAME: &str = "师徒";
const PLOT_TITLE: &str = "伏笔·第七秒的暗号";
const REVISION_TITLE: &str = "第一章·修订快照";
const LEXICON_TERM: &str = "灯塔暗号";

/// 灌入覆盖各业务表的测试记录。
///
/// 用显式 SQL 而不是 db 模块的 API：迁移测试要钉住的是**列名与表结构**，
/// 走 API 会让 fixture 随 API 重构而悄悄变形。
fn seed_business_data(conn: &Connection, with_lexicon: bool) {
    conn.execute(
        "INSERT INTO novel (id, title, synopsis) VALUES ('n1', ?1, '迁移验证用梗概')",
        [NOVEL_TITLE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO volume (id, novel_id, title, summary, sort_order)
         VALUES ('v1', 'n1', '第一卷', '', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chapter (id, novel_id, volume_id, title, summary, sort_order)
         VALUES ('c1', 'n1', 'v1', '第一章', '', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE chapter SET content = ?1, content_plain = ?2, word_count = 28 WHERE id = 'c1'",
        rusqlite::params![CHAPTER_HTML, CHAPTER_BODY],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO worldview_entry (id, novel_id, title, content)
         VALUES ('w1', 'n1', ?1, '守望者世代居住在灯塔里')",
        [WORLDVIEW_TITLE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO character (id, novel_id, name) VALUES ('ch1', 'n1', ?1)",
        [CHAR_A],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO character (id, novel_id, name) VALUES ('ch2', 'n1', ?1)",
        [CHAR_B],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO character_relation (id, novel_id, from_char_id, to_char_id, relation_name)
         VALUES ('r1', 'n1', 'ch1', 'ch2', ?1)",
        [RELATION_NAME],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO plot_item (id, novel_id, title, chapter_id)
         VALUES ('p1', 'n1', ?1, 'c1')",
        [PLOT_TITLE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO chapter_revision
             (id, chapter_id, novel_id, title, content_plain, word_count, revision_type)
         VALUES ('rev1', 'c1', 'n1', ?1, ?2, 28, 'manual')",
        rusqlite::params![REVISION_TITLE, CHAPTER_BODY],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO setting_change (id, novel_id, target_type, target_id, target_title)
         VALUES ('sc1', 'n1', 'character', 'ch1', ?1)",
        [CHAR_A],
    )
    .unwrap();
    if with_lexicon {
        conn.execute(
            "INSERT INTO novel_lexicon (id, novel_id, term, category, frequency)
             VALUES ('lex1', 'n1', ?1, 'custom', 3)",
            [LEXICON_TERM],
        )
        .unwrap();
    }
}

/// 逐条核对业务数据在迁移后仍然原样存在。
fn verify_business_data(conn: &Connection, expect_lexicon: bool) {
    assert_eq!(count(conn, "novel"), 1, "作品记录必须保留");
    let title: String = conn
        .query_row("SELECT title FROM novel WHERE id = 'n1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(title, NOVEL_TITLE);

    assert_eq!(count(conn, "volume"), 1);
    assert_eq!(count(conn, "chapter"), 1);

    // 正文必须逐字保留 —— 这是作者最不能接受丢失的东西
    let (html, plain, words): (String, String, i64) = conn
        .query_row(
            "SELECT content, content_plain, word_count FROM chapter WHERE id = 'c1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(html, CHAPTER_HTML, "章节 HTML 正文必须逐字不变");
    assert_eq!(plain, CHAPTER_BODY, "章节纯文本正文必须逐字不变");
    assert_eq!(words, 28);

    assert_eq!(count(conn, "worldview_entry"), 1);
    let wv: String = conn
        .query_row("SELECT title FROM worldview_entry WHERE id = 'w1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(wv, WORLDVIEW_TITLE);

    assert_eq!(count(conn, "character"), 2);
    assert_eq!(count(conn, "character_relation"), 1);
    let rel: String = conn
        .query_row(
            "SELECT relation_name FROM character_relation WHERE id = 'r1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rel, RELATION_NAME);

    assert_eq!(count(conn, "plot_item"), 1);
    let plot: String = conn
        .query_row("SELECT title FROM plot_item WHERE id = 'p1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(plot, PLOT_TITLE);

    assert_eq!(count(conn, "chapter_revision"), 1, "章节历史版本必须保留");
    let rev: String = conn
        .query_row(
            "SELECT content_plain FROM chapter_revision WHERE id = 'rev1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rev, CHAPTER_BODY, "历史版本正文必须逐字不变");

    assert_eq!(count(conn, "setting_change"), 1);

    if expect_lexicon {
        assert_eq!(count(conn, "novel_lexicon"), 1, "词库记录必须保留");
        let term: String = conn
            .query_row("SELECT term FROM novel_lexicon WHERE id = 'lex1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(term, LEXICON_TERM);
    }
}

/// 造一个处于指定历史状态的库（含业务数据）。
fn historical_fixture(tag: &str, state: HistoricalState) -> PathBuf {
    let dir = temp_dir(tag);
    let db_path = dir.join("easy-writing.db");

    let pool = db::init_db(dir.clone()).unwrap();
    {
        let conn = pool.get().unwrap();
        seed_business_data(&conn, true);

        match state {
            HistoricalState::NoVersionTable => {
                // ≤ v0.3.3 的库根本没有这张表，也没有 V004 的列
                conn.execute("DROP TABLE schema_version", []).unwrap();
                drop_v007_objects(&conn);
                drop_v006_objects(&conn);
                drop_v005_objects(&conn);
                drop_v004_columns(&conn);
            }
            HistoricalState::V1 => {
                for idx in V003_INDEXES {
                    conn.execute(&format!("DROP INDEX IF EXISTS {idx}"), []).unwrap();
                }
                for obj in V002_ONLY_OBJECTS {
                    if obj.starts_with("idx_") {
                        conn.execute(&format!("DROP INDEX IF EXISTS {obj}"), [])
                            .unwrap();
                    } else {
                        conn.execute(&format!("DROP TABLE IF EXISTS {obj}"), [])
                            .unwrap();
                    }
                }
                drop_v007_objects(&conn);
                drop_v006_objects(&conn);
                drop_v005_objects(&conn);
                drop_v004_columns(&conn);
                conn.execute("DELETE FROM schema_version WHERE version <> 1", [])
                    .unwrap();
            }
            HistoricalState::V2 => {
                for idx in V003_INDEXES {
                    conn.execute(&format!("DROP INDEX IF EXISTS {idx}"), []).unwrap();
                }
                drop_v007_objects(&conn);
                drop_v006_objects(&conn);
                drop_v005_objects(&conn);
                drop_v004_columns(&conn);
                conn.execute("DELETE FROM schema_version WHERE version > 2", [])
                    .unwrap();
            }
            HistoricalState::V3 => {
                // V004 只加列：把 pinned 列物理删掉，表结构才等于真实 v3 库
                drop_v007_objects(&conn);
                drop_v006_objects(&conn);
                drop_v005_objects(&conn);
                for table in ["character", "worldview_entry", "plot_item"] {
                    conn.execute(&format!("ALTER TABLE {table} DROP COLUMN pinned"), [])
                        .unwrap();
                }
                conn.execute("DELETE FROM schema_version WHERE version > 3", [])
                    .unwrap();
            }
            HistoricalState::V4 => {
                // T06 之后、T09 之前的库：有 pinned，但没有 FTS 与 batch_id
                drop_v007_objects(&conn);
                drop_v006_objects(&conn);
                drop_v005_objects(&conn);
                conn.execute("DELETE FROM schema_version WHERE version > 4", [])
                    .unwrap();
            }
            HistoricalState::V5 => {
                // T09 之后、T10 之前的库：有 FTS，但没有去重锚点列
                drop_v007_objects(&conn);
                drop_v006_objects(&conn);
                conn.execute("DELETE FROM schema_version WHERE version > 5", [])
                    .unwrap();
            }
            HistoricalState::V6 => {
                // PR-10 之后、PR-11 之前的库：V001–V006 齐全，只缺 V007 的忽略表
                drop_v007_objects(&conn);
                conn.execute("DELETE FROM schema_version WHERE version > 6", [])
                    .unwrap();
            }
        }
    }
    drop(pool);

    // 前置断言：fixture 必须真的处于目标状态，否则后面的用例会变成空跑
    let conn = Connection::open(&db_path).unwrap();
    match state {
        HistoricalState::NoVersionTable => {
            assert!(
                !table_exists(&conn, "schema_version"),
                "fixture 应当没有 schema_version 表"
            );
            assert_pinned_columns_absent(&conn);
            assert_v005_objects_absent(&conn);
            assert_v006_objects_absent(&conn);
            assert_v007_objects_absent(&conn);
        }
        HistoricalState::V1 => {
            assert_eq!(versions(&conn), vec![1], "fixture 应处于 v1，实际 {:?}", versions(&conn));
            assert!(
                !table_exists(&conn, "novel_lexicon"),
                "v1 状态不应有 V002 才创建的 novel_lexicon"
            );
            assert!(
                !index_exists(&conn, "idx_asset_hash"),
                "v1 状态不应有 V003 的索引"
            );
            assert_pinned_columns_absent(&conn);
            assert_v005_objects_absent(&conn);
            assert_v006_objects_absent(&conn);
            assert_v007_objects_absent(&conn);
        }
        HistoricalState::V2 => {
            assert_eq!(versions(&conn), vec![1, 2], "fixture 应处于 v2");
            assert!(
                table_exists(&conn, "novel_lexicon"),
                "v2 状态应已有 V002 创建的 novel_lexicon"
            );
            for idx in V003_INDEXES {
                assert!(!index_exists(&conn, idx), "v2 状态不应有 V003 索引 {idx}");
            }
            assert_pinned_columns_absent(&conn);
            assert_v005_objects_absent(&conn);
            assert_v006_objects_absent(&conn);
            assert_v007_objects_absent(&conn);
        }
        HistoricalState::V3 => {
            assert_eq!(versions(&conn), vec![1, 2, 3], "fixture 应处于 v3");
            assert_pinned_columns_absent(&conn);
            assert_v005_objects_absent(&conn);
            assert_v006_objects_absent(&conn);
            assert_v007_objects_absent(&conn);
        }
        HistoricalState::V4 => {
            assert_eq!(versions(&conn), vec![1, 2, 3, 4], "fixture 应处于 v4");
            for table in ["character", "worldview_entry", "plot_item"] {
                assert!(
                    column_exists(&conn, table, "pinned"),
                    "v4 状态应已有 V004 的 pinned 列（{table}）"
                );
            }
            assert_v005_objects_absent(&conn);
            assert_v006_objects_absent(&conn);
            assert_v007_objects_absent(&conn);
        }
        HistoricalState::V5 => {
            assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5], "fixture 应处于 v5");
            assert!(
                table_exists(&conn, "chapter_fts"),
                "v5 状态应已有 V005 的 chapter_fts"
            );
            assert_v006_objects_absent(&conn);
            assert_v007_objects_absent(&conn);
        }
        HistoricalState::V6 => {
            assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6], "fixture 应处于 v6");
            assert!(
                table_exists(&conn, "chapter_fts"),
                "v6 状态应已有 V005 的 chapter_fts"
            );
            assert_v006_objects_present(&conn);
            assert_v007_objects_absent(&conn);
        }
    }
    drop(conn);
    dir
}

#[test]
fn v1_database_upgrades_continuously_to_v6_keeping_all_data() {
    // v1 → v2 → v3 → v4 → v5 → v6 的完整阶梯。这是任务书要求的「连续升级、不跳过任何 migration」。
    let dir = historical_fixture("ladder_v1", HistoricalState::V1);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(
        schema_version(&conn),
        db::BASELINE_VERSION,
        "升级后必须到达基线版本"
    );
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7], "七级版本记录都必须存在");

    // V002 的产物必须被真正创建（证明 V002 没有被跳过）
    assert!(
        table_exists(&conn, "novel_lexicon"),
        "V002 创建的 novel_lexicon 必须存在"
    );
    assert!(index_exists(&conn, "idx_lexicon_novel_term"));
    assert!(index_exists(&conn, "idx_prompt_template_scope"));

    // V003 的产物必须被真正创建（证明 V003 没有被跳过）
    for idx in V003_INDEXES {
        assert!(index_exists(&conn, idx), "V003 索引 {idx} 必须存在");
    }

    // V004 的产物必须被真正创建（证明 V004 没有被跳过）
    for table in ["character", "worldview_entry", "plot_item"] {
        assert!(
            column_exists(&conn, table, "pinned"),
            "V004 为 {table} 添加的 pinned 列必须存在"
        );
    }

    // V005 的产物必须被真正创建（证明 V005 没有被跳过），且存量已回填
    assert_v005_objects_present(&conn);
    assert_v006_objects_present(&conn);
    assert_v007_objects_present(&conn);

    // v1 库里本来就没有词库数据（表都不存在），因此只验证表已就位
    verify_business_data(&conn, false);
    assert_eq!(
        count(&conn, "novel_lexicon"),
        0,
        "v1 库没有词库表，迁移后应为空表而不是凭空造出数据"
    );
}

#[test]
fn v2_database_upgrades_to_v7_keeping_all_data() {
    // v0.3.4 用户升级到新版本的真实路径：补 V003 + V004 + V005 + V006 + V007。
    let dir = historical_fixture("ladder_v2", HistoricalState::V2);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7]);
    for idx in V003_INDEXES {
        assert!(index_exists(&conn, idx), "V003 索引 {idx} 必须被补上");
    }
    for table in ["character", "worldview_entry", "plot_item"] {
        assert!(
            column_exists(&conn, table, "pinned"),
            "V004 为 {table} 添加的 pinned 列必须被补上"
        );
    }
    assert_v005_objects_present(&conn);
    assert_v006_objects_present(&conn);
    assert_v007_objects_present(&conn);
    // v2 库里词库数据是存在的，必须原样保留
    verify_business_data(&conn, true);
}

#[test]
fn v3_database_upgrades_to_v7_keeping_all_data() {
    // v0.3.5–v0.3.8 用户升级的真实路径：补 V004（pinned）+ V005（FTS5）+ V006（去重锚点）+ V007。
    let dir = historical_fixture("ladder_v3", HistoricalState::V3);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7]);
    for table in ["character", "worldview_entry", "plot_item"] {
        assert!(
            column_exists(&conn, table, "pinned"),
            "V004 为 {table} 添加的 pinned 列必须被补上"
        );
        // pinned 默认 0：升级不得把任何存量设定变成「恒入」
        let pinned_sum: i64 = conn
            .query_row(
                &format!("SELECT COALESCE(SUM(pinned), 0) FROM {table}"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pinned_sum, 0, "{table} 的存量记录 pinned 必须全为 0");
    }
    assert_v005_objects_present(&conn);
    assert_v006_objects_present(&conn);
    assert_v007_objects_present(&conn);
    // v3 库里的全部业务数据（含词库）必须原样保留
    verify_business_data(&conn, true);
}

#[test]
fn v4_database_upgrades_to_v7_keeping_all_data() {
    // T06 之后、T09 之前的用户升级的真实路径：补 V005（FTS5 + batch_id）+ V006 + V007。
    let dir = historical_fixture("ladder_v4", HistoricalState::V4);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7]);
    assert_v005_objects_present(&conn);
    assert_v006_objects_present(&conn);
    assert_v007_objects_present(&conn);
    // 升级不得改动任何既有正文（索引是派生数据，正文才是真相）
    verify_business_data(&conn, true);
}

#[test]
fn v5_database_upgrades_to_v7_keeping_all_data() {
    // PR-09 之后、PR-10 之前的用户升级路径：补 V006（去重锚点列）+ V007。
    let dir = historical_fixture("ladder_v5", HistoricalState::V5);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7]);
    assert_v006_objects_present(&conn);
    // V005 的全文索引必须原样保留（V006/V007 只加列建表，不得碰索引）
    assert_v005_objects_present(&conn);
    assert_v007_objects_present(&conn);
    verify_business_data(&conn, true);
}

#[test]
fn v6_database_upgrades_to_v7_keeping_all_data() {
    // PR-10 之后、PR-11 之前的用户升级路径：只需要补 V007（本章忽略记录表）。
    let dir = historical_fixture("ladder_v6", HistoricalState::V6);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7]);
    assert_v006_objects_present(&conn);
    assert_v007_objects_present(&conn);
    // V005 的全文索引必须原样保留（V007 只是新建一张辅助表）
    assert_v005_objects_present(&conn);
    verify_business_data(&conn, true);
}

#[test]
fn pre_ladder_database_without_version_table_upgrades_to_v7() {
    // ≤ v0.3.3 的库连 schema_version 表都没有 —— 这是最老、也是数量最多的
    // 一类真实用户库。它必须能一路升到 v7 且数据完好。
    let dir = historical_fixture("ladder_legacy", HistoricalState::NoVersionTable);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(schema_version(&conn), db::BASELINE_VERSION);
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7]);
    for idx in V003_INDEXES {
        assert!(index_exists(&conn, idx));
    }
    assert_v007_objects_present(&conn);
    verify_business_data(&conn, true);
}

#[test]
fn completed_migrations_are_not_rerun_on_restart() {
    // 「不重复执行有副作用的迁移」的可观测证据：
    // 已应用版本的 applied_at 必须一字不变，版本集合也不能增长。
    let dir = historical_fixture("ladder_idempotent", HistoricalState::V2);

    let first_applied_at = {
        let pool = db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        (
            applied_at(&conn, 1),
            applied_at(&conn, 2),
            applied_at(&conn, 3),
            applied_at(&conn, 4),
        )
    };
    assert!(
        first_applied_at.0.is_some()
            && first_applied_at.2.is_some()
            && first_applied_at.3.is_some()
    );

    for round in 0..2 {
        let pool = db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(
            schema_version(&conn),
            db::BASELINE_VERSION,
            "第 {} 次重启后版本号不应增长",
            round + 1
        );
        assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7], "不应出现重复版本记录");
        assert_eq!(
            (
                applied_at(&conn, 1),
                applied_at(&conn, 2),
                applied_at(&conn, 3),
                applied_at(&conn, 4)
            ),
            first_applied_at,
            "第 {} 次重启不应重新执行已完成的迁移（applied_at 变了）",
            round + 1
        );
        verify_business_data(&conn, true);
    }
}

#[test]
fn v2_migration_failure_does_not_mark_version_as_applied() {
    // V002 的第一条语句是 `ALTER TABLE setting_change ...`。
    // 把这张表删掉就能触发一个真实的、非 mock 的迁移失败。
    let dir = historical_fixture("ladder_v2_fail", HistoricalState::V1);
    {
        let conn = Connection::open(dir.join("easy-writing.db")).unwrap();
        conn.execute("DROP TABLE setting_change", []).unwrap();
    }

    let err = db::init_db(dir.clone())
        .expect_err("V002 失败时 init_db 必须返回错误，不能静默成功");
    assert!(
        err.to_string().contains("setting_change"),
        "错误必须能定位到具体原因，实际：{err}"
    );

    let conn = Connection::open(dir.join("easy-writing.db")).unwrap();
    let vs = versions(&conn);
    assert!(
        !vs.contains(&2),
        "迁移失败绝不能把版本标记成已成功，实际版本记录：{vs:?}"
    );
    assert!(
        !vs.contains(&3),
        "V002 失败后不应继续执行 V003，实际版本记录：{vs:?}"
    );
}

#[test]
fn v3_migration_failure_does_not_mark_version_as_applied() {
    // V003 会给 plot_item 建索引；删掉这张表即可触发真实失败。
    let dir = historical_fixture("ladder_v3_fail", HistoricalState::V2);
    {
        let conn = Connection::open(dir.join("easy-writing.db")).unwrap();
        conn.execute("DROP TABLE plot_item", []).unwrap();
    }

    let err = db::init_db(dir.clone())
        .expect_err("V003 失败时 init_db 必须返回错误");
    assert!(
        err.to_string().contains("plot_item"),
        "错误必须能定位到具体原因，实际：{err}"
    );

    let conn = Connection::open(dir.join("easy-writing.db")).unwrap();
    let vs = versions(&conn);
    assert!(
        !vs.contains(&3),
        "V003 失败绝不能记录为已应用，实际版本记录：{vs:?}"
    );
    assert!(vs.contains(&2), "已成功的 V002 记录不应被抹掉");
    // 库本身仍然可读：失败不能把它变成砖头
    assert_eq!(count(&conn, "novel"), 1, "迁移失败不得损坏已有数据");
}

#[test]
fn higher_version_database_is_never_downgraded_by_older_client() {
    // 作者在新机器上升级到未来版本、又把同一个数据目录拿回旧版本打开。
    // 旧客户端可以拒绝启动，但绝不能把高版本库降级或覆盖。
    //
    // fixture 从**完整的 v7 状态**再加 version 8，而不是从 v3 直接跳到 8：
    // 真实的高版本库必然已经跑完了 1→…→7，`{1,2,3,8}` 那种缺号的库
    // 现实中不会出现，用它当 fixture 会测到一个假场景。
    let dir = temp_dir("ladder_future");
    {
        let pool = db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        seed_business_data(&conn, true);
        assert_eq!(
            versions(&conn),
            vec![1, 2, 3, 4, 5, 6, 7],
            "前置：必须是完整 v7 库"
        );

        conn.execute("INSERT INTO schema_version (version) VALUES (8)", [])
            .unwrap();
        // 未来版本新增的对象：当前客户端不认识，但必须原样留着
        conn.execute(
            "CREATE TABLE future_only_table (id TEXT PRIMARY KEY, note TEXT NOT NULL DEFAULT '')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO future_only_table (id, note) VALUES ('f1', 'written by a newer client')",
            [],
        )
        .unwrap();
    }

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    assert_eq!(
        schema_version(&conn),
        8,
        "旧客户端不得把 schema 回退到 {}",
        db::BASELINE_VERSION
    );
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(
        table_exists(&conn, "future_only_table"),
        "未来版本新增的表不得被删除"
    );
    let note: String = conn
        .query_row("SELECT note FROM future_only_table WHERE id = 'f1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(note, "written by a newer client", "未来版本写入的数据必须完好");
    verify_business_data(&conn, true);
}

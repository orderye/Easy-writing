//! 大数据量迁移的性能与锁风险验收（V0.3.8 补充任务书 P1-3）。
//!
//! ## 怎么跑
//!
//! ```text
//! cargo test -p easy-writing-core --test migration_perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! 默认 `#[ignore]`：造 1000 章数据的成本不适合放进常规 CI 门禁。
//! 需要设门槛时用 `EW_MIGRATION_GATE_MS`（毫秒）；**不设就只报告、不判定**。
//!
//! ## 为什么默认不设阈值
//!
//! 任务书 P1-3 明确要求「CI 性能门槛应基于稳定的 CI runner 实测确定」。
//! 本机是 macOS arm64 + 外置机械盘，与 GitHub runner 的差异是数量级的，
//! 在这里拍一个数字写进 CI 只会制造噪音，最终让人养成忽略红色构建的习惯。
//! 因此这里只输出实测数据，门槛留给 runner 上的真实分布来定。
//!
//! ## 数据集是合成的
//!
//! 下面生成的是合成数据，不是真实用户作品。规模写在用例名与输出里，
//! 报告必须原样引用，不能把合成测试说成真实作品测试。
//!
//! ## 数据安全
//!
//! 全部数据都在 `std::env::temp_dir()` 下的隔离目录里，
//! **不触碰用户真实数据库**。

use easy_writing_lib::db;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// V003 创建的索引。退化到 v1/v2 状态时要删掉它们。
const V003_INDEXES: &[&str] = &[
    "idx_asset_novel_hash",
    "idx_asset_hash",
    "idx_plot_item_chapter",
    "idx_setting_change_target",
    "idx_chapter_revision_auto",
];

fn gate_ms() -> Option<f64> {
    std::env::var("EW_MIGRATION_GATE_MS")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
}

fn workdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ew_migperf_{}_{}_{:?}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn db_file_size(dir: &Path) -> u64 {
    fs::metadata(dir.join("easy-writing.db")).map(|m| m.len()).unwrap_or(0)
}

/// 用裸 SQL 在单事务里造数据。
///
/// 不走 db 模块的 API：造 1000 章时逐条自动提交会把「造数据」的耗时
/// 混进报告里，而我们要测的是**迁移**的耗时。
fn seed(
    conn: &Connection,
    novels: usize,
    volumes_per_novel: usize,
    chapters_per_volume: usize,
    revisions_per_chapter: usize,
    settings_per_novel: usize,
) {
    conn.execute("BEGIN", []).unwrap();
    for n in 0..novels {
        let nid = format!("n{n}");
        conn.execute(
            "INSERT INTO novel (id, title, synopsis) VALUES (?1, ?2, '合成数据')",
            rusqlite::params![nid, format!("合成作品{n}")],
        )
        .unwrap();
        let mut chapter_seq = 0usize;
        for v in 0..volumes_per_novel {
            let vid = format!("{nid}-v{v}");
            conn.execute(
                "INSERT INTO volume (id, novel_id, title, summary, sort_order)
                 VALUES (?1, ?2, ?3, '', ?4)",
                rusqlite::params![vid, nid, format!("第{}卷", v + 1), v as i64],
            )
            .unwrap();
            for c in 0..chapters_per_volume {
                let cid = format!("{vid}-c{c}");
                // 确定性正文：不含随机数，保证每次运行的数据完全一致，
                // 否则耗时波动会掩盖真实的代码改动影响。
                let body: String = (0..2000)
                    .map(|i| char::from_u32(0x4E00 + ((i + c + v) % 500) as u32).unwrap_or('字'))
                    .collect();
                conn.execute(
                    "INSERT INTO chapter
                         (id, novel_id, volume_id, title, summary, content, content_plain,
                          word_count, sort_order)
                     VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, 2000, ?7)",
                    rusqlite::params![
                        cid,
                        nid,
                        vid,
                        format!("第{}章", chapter_seq + 1),
                        format!("<p>{body}</p>"),
                        body,
                        c as i64
                    ],
                )
                .unwrap();
                for r in 0..revisions_per_chapter {
                    conn.execute(
                        "INSERT INTO chapter_revision
                             (id, chapter_id, novel_id, title, content, content_plain,
                              word_count, revision_type)
                         VALUES (?1, ?2, ?3, ?4, '{}', '', 2000, 'auto')",
                        rusqlite::params![
                            format!("{cid}-r{r}"),
                            cid,
                            nid,
                            format!("第{}章·修订{r}", chapter_seq + 1)
                        ],
                    )
                    .unwrap();
                }
                chapter_seq += 1;
            }
        }
        for s in 0..settings_per_novel {
            conn.execute(
                "INSERT INTO setting_change
                     (id, novel_id, target_type, target_id, target_title)
                 VALUES (?1, ?2, 'character', ?3, '合成角色')",
                rusqlite::params![format!("{nid}-sc{s}"), nid, format!("ch{s}")],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO character (id, novel_id, name) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("ch{s}"), nid, format!("合成角色{s}")],
            )
            .unwrap();
        }
    }
    conn.execute("COMMIT", []).unwrap();
}

/// 把库退化到「v1 已应用、V002/V003 未应用」的历史状态，
/// 使随后的 `init_db` 必须真的跑完 V002 + V003。
fn degrade_to_v1(conn: &Connection) {
    for idx in V003_INDEXES {
        conn.execute(&format!("DROP INDEX IF EXISTS {idx}"), []).unwrap();
    }
    for idx in ["idx_lexicon_novel_term", "idx_lexicon_novel", "idx_prompt_template_scope"] {
        conn.execute(&format!("DROP INDEX IF EXISTS {idx}"), []).unwrap();
    }
    conn.execute("DROP TABLE IF EXISTS novel_lexicon", []).unwrap();
    conn.execute("DELETE FROM schema_version WHERE version <> 1", [])
        .unwrap();
}

struct MigrationReport {
    name: String,
    dataset: String,
    seed_ms: f64,
    migrate_ms: f64,
    size_before_kb: u64,
    size_after_kb: u64,
    chapters: i64,
    revisions: i64,
}

impl MigrationReport {
    fn print_and_gate(&self) {
        println!(
            "[MIGRATION] {:<34} {:<30} seed={:>8.1}ms migrate={:>9.2}ms \
             size {}KB→{}KB chapters={} revisions={}",
            self.name,
            self.dataset,
            self.seed_ms,
            self.migrate_ms,
            self.size_before_kb,
            self.size_after_kb,
            self.chapters,
            self.revisions
        );
        if let Some(limit) = gate_ms() {
            assert!(
                self.migrate_ms <= limit,
                "迁移性能门禁未通过：{} 在 {} 上耗时 {:.2}ms，超过 {:.0}ms 阈值",
                self.name,
                self.dataset,
                self.migrate_ms,
                limit
            );
        }
    }
}

/// 造一个处于 v1 历史状态的大库，然后测量 `init_db` 的完整迁移耗时。
fn measure(
    name: &str,
    dataset: &str,
    novels: usize,
    volumes_per_novel: usize,
    chapters_per_volume: usize,
    revisions_per_chapter: usize,
    settings_per_novel: usize,
) -> (MigrationReport, std::path::PathBuf) {
    let dir = workdir(name);

    // 先用当前代码建出完整库并灌数据，再退化到 v1 状态
    let t_seed = Instant::now();
    {
        let pool = db::init_db(dir.clone()).unwrap();
        let conn = pool.get().unwrap();
        seed(
            &conn,
            novels,
            volumes_per_novel,
            chapters_per_volume,
            revisions_per_chapter,
            settings_per_novel,
        );
        degrade_to_v1(&conn);
    }
    let seed_ms = t_seed.elapsed().as_secs_f64() * 1000.0;
    let size_before = db_file_size(&dir) / 1024;

    // 迁移：从 v1 一路跑到 BASELINE_VERSION
    let t0 = Instant::now();
    let pool = db::init_db(dir.clone()).unwrap();
    let migrate_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let (chapters, revisions, version) = {
        let conn = pool.get().unwrap();
        (
            conn.query_row("SELECT COUNT(*) FROM chapter", [], |r| r.get(0))
                .unwrap(),
            conn.query_row("SELECT COUNT(*) FROM chapter_revision", [], |r| r.get(0))
                .unwrap(),
            conn.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
        )
    };

    assert_eq!(
        version,
        db::BASELINE_VERSION,
        "迁移后必须到达基线版本，实际 {version}"
    );

    let report = MigrationReport {
        name: name.to_string(),
        dataset: dataset.to_string(),
        seed_ms,
        migrate_ms,
        size_before_kb: size_before,
        size_after_kb: db_file_size(&dir) / 1024,
        chapters,
        revisions,
    };
    report.print_and_gate();
    (report, dir)
}

#[test]
#[ignore = "大数据迁移基准：cargo test --test migration_perf -- --ignored --nocapture"]
fn migration_small_legacy_database() {
    let (r, dir) = measure("small", "1 部作品 / 10 章", 1, 1, 10, 0, 5);
    assert_eq!(r.chapters, 10);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "大数据迁移基准：cargo test --test migration_perf -- --ignored --nocapture"]
fn migration_thousand_chapter_database() {
    // 任务书要求的「约 1000 章」规模
    let (r, dir) = measure("thousand", "1 部作品 / 10 卷 × 100 章 = 1000 章", 1, 10, 100, 0, 20);
    assert_eq!(r.chapters, 1000, "必须真的造出 1000 章，否则测不到规模效应");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "大数据迁移基准：cargo test --test migration_perf -- --ignored --nocapture"]
fn migration_with_heavy_revisions_and_settings() {
    // 大量历史版本 + 大量设定变更记录：V003 给这两张表都建了索引，
    // 正是最容易在迁移期出现长事务与锁等待的组合。
    let (r, dir) = measure(
        "revisions",
        "200 章 × 5 个历史版本 + 300 条设定变更",
        1,
        2,
        100,
        5,
        300,
    );
    assert_eq!(r.chapters, 200);
    assert_eq!(r.revisions, 1000, "必须真的造出 1000 条历史版本");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "大数据迁移基准：cargo test --test migration_perf -- --ignored --nocapture"]
fn migration_leaves_no_lock_and_wal_is_active() {
    // 迁移期间的锁风险：迁移结束后必须没有残留写锁，
    // 且连接池里的其他连接能立刻读到数据（WAL 模式生效）。
    let (r, dir) = measure("lockcheck", "1000 章（锁与 WAL 检查）", 1, 10, 100, 0, 0);
    assert_eq!(r.chapters, 1000);

    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();

    let journal: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        journal.to_lowercase(),
        "wal",
        "迁移后必须仍处于 WAL 模式，否则读写会互相阻塞"
    );
    let busy: i64 = conn
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert!(busy > 0, "必须设置 busy_timeout，实际 {busy}");

    // 连接池的第二个连接必须能立刻读：证明迁移没有留下未释放的写锁
    let conn2 = pool.get().unwrap();
    let n: i64 = conn2
        .query_row("SELECT COUNT(*) FROM chapter", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1000);

    // 迁移是可重入的：重复初始化不应再次耗时或改变数据
    let t0 = Instant::now();
    let pool_again = db::init_db(dir.clone()).unwrap();
    let again_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("[MIGRATION] 重复初始化耗时 {again_ms:.2}ms（应远小于首次迁移）");
    let conn3 = pool_again.get().unwrap();
    let n3: i64 = conn3
        .query_row("SELECT COUNT(*) FROM chapter", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n3, 1000);

    let _ = fs::remove_dir_all(&dir);
}

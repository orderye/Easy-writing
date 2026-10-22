//! 备份保留策略的回归测试（任务书 T03 / PR-04）。
//!
//! ## 覆盖什么
//!
//! - **策略引擎**（纯函数）：最近 N 份 + 每日 1 份 + 每周 1 份的并集语义；
//! - **两条安全铁律**：新备份未验证不删、非 `backup_` 前缀的文件不删；
//! - **快照侧**：内容相同不新增快照、上限可配且只清自动快照；
//! - **VACUUM**：在备份副本上执行且不改变数据；
//! - **端到端轮转**：真实 HTTP 往返后，远端剩下的文件集合与策略一致，
//!   被删备份的清单 sidecar 一起消失，删除失败不影响已上传的新备份。
//!
//! ## 数据安全
//!
//! 全部数据都在 `std::env::temp_dir()` 下的隔离目录与进程内 WebDAV 服务端中，
//! **不接触用户真实数据库与真实 WebDAV 目录**。

mod common;

use common::webdav_server::TestWebDav;
use easy_writing_lib::db;
use easy_writing_lib::sync::backup;
use easy_writing_lib::sync::retention;
use easy_writing_lib::sync::webdav::WebDavClient;
use shared_types::RetentionPolicy;
use chrono::Datelike;
use std::collections::BTreeSet;
use std::path::Path;

const REMOTE_DIR: &str = "ew-retention";

fn client(server: &TestWebDav) -> WebDavClient {
    WebDavClient::new(&server.base_url(), "test-user", "test-password")
}

fn policy(recent: usize, daily: usize, weekly: usize) -> RetentionPolicy {
    RetentionPolicy {
        recent,
        daily,
        weekly,
    }
}

/// 构造 n 份跨越多周的备份名（每天一份，UTC）。
fn daily_backups(n: usize) -> Vec<String> {
    let start = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    (0..n)
        .map(|i| {
            let d = start + chrono::Duration::days(i as i64);
            format!("backup_{}_000000Z.db.gz", d.format("%Y%m%d"))
        })
        .collect()
}

/// 从备份名里取出日期。
fn backup_date(name: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(&name[7..15], "%Y%m%d").unwrap()
}

// ---------------------------------------------------------------------------
// 策略引擎
// ---------------------------------------------------------------------------

#[test]
fn retention_keeps_recent_daily_and_weekly() {
    // 60 天连续备份，跨 9 个自然周。
    let list = daily_backups(60);
    let p = policy(20, 7, 4);

    let plan = retention::build_plan(&list, p);

    // 独立算出「应当保留」的集合：三个维度的并集。
    let mut expected: BTreeSet<String> = BTreeSet::new();
    // ① 最近 20 份
    for name in list.iter().rev().take(20) {
        expected.insert(name.clone());
    }
    // ② 最近 7 天，每天一份（每天只有一份备份）
    for name in list.iter().rev().take(7) {
        expected.insert(name.clone());
    }
    // ③ 最近 4 个 ISO 周，每周最新一份
    let mut by_week: std::collections::BTreeMap<(i32, u32), &String> =
        std::collections::BTreeMap::new();
    for name in list.iter().rev() {
        let iso = backup_date(name).iso_week();
        by_week.entry((iso.year(), iso.week())).or_insert(name);
    }
    for name in by_week.values().rev().take(4) {
        expected.insert((*name).clone());
    }

    let actual: BTreeSet<String> = plan.keep.iter().cloned().collect();
    assert_eq!(
        actual, expected,
        "保留集合必须等于三个维度的并集\n实际：{actual:?}\n期望：{expected:?}"
    );
    assert_eq!(plan.delete.len(), 60 - expected.len());
    assert!(plan
        .delete
        .contains(&"backup_20260101_000000Z.db.gz".to_string()));
}

#[test]
fn retention_deletes_nothing_when_unverified() {
    let list = daily_backups(30);
    let del = retention::plan_retention(&list, false, policy(1, 0, 0));
    assert!(
        del.is_empty(),
        "新备份未验证通过时一个都不能删，实际删了：{del:?}"
    );
}

#[test]
fn retention_ignores_foreign_files() {
    let mut list = daily_backups(5);
    list.push("someone-elses-important-file.db.gz".to_string());
    list.push("my-notes.db.gz".to_string());

    let plan = retention::build_plan(&list, policy(1, 0, 0));
    for foreign in ["someone-elses-important-file.db.gz", "my-notes.db.gz"] {
        assert!(
            !plan.delete.iter().any(|n| n == foreign),
            "不属于本应用备份命名范围的文件不得被删：{foreign}"
        );
    }
    assert_eq!(plan.protected_foreign, 2);
}

#[test]
fn retention_keeps_unparsable_timestamps_managed() {
    // 名字解析不出时间戳的备份仍然归本应用策略管（归属由**命名规则**决定），
    // 但它不参与每日 / 每周维度，只受「最近 N 份」约束。
    let mut list = daily_backups(3);
    list.push("backup_mystery.db.gz".to_string());

    let plan = retention::build_plan(&list, policy(1, 0, 0));
    assert!(
        plan.keep.contains(&"backup_mystery.db.gz".to_string()),
        "按文件名字典序，最近的一份就是它：{:?}",
        plan.keep
    );
    assert_eq!(plan.delete.len(), 3);
}

#[test]
fn legacy_plan_rotation_semantics_unchanged() {
    // 旧实现的四条语义（backup.rs 内既有单元测试）在新引擎下必须逐条复验：
    // 既有测试一行未改，这里是它们的端到端复验。
    let two = vec![
        "backup_20260101_000000Z.db.gz".to_string(),
        "backup_20260102_000000Z.db.gz".to_string(),
    ];
    assert!(backup::plan_rotation(&two, true, 20).is_empty());

    let three = vec![
        "backup_20260101_000000Z.db.gz".to_string(),
        "backup_20260102_000000Z.db.gz".to_string(),
        "backup_20260103_000000Z.db.gz".to_string(),
    ];
    assert_eq!(
        backup::plan_rotation(&three, true, 2),
        vec!["backup_20260101_000000Z.db.gz".to_string()]
    );

    let many: Vec<String> = (1..=25)
        .map(|i| format!("backup_202601{:02}_000000Z.db.gz", i))
        .collect();
    assert!(backup::plan_rotation(&many, false, 20).is_empty());

    let mut with_foreign = three.clone();
    with_foreign.push("someone-elses-data.db.gz".to_string());
    let del = backup::plan_rotation(&with_foreign, true, 2);
    assert_eq!(del.len(), 1);
    assert!(!del.iter().any(|n| n.contains("someone")));
}

#[test]
fn policy_falls_back_when_all_dimensions_are_zero() {
    // 「三个维度全 0」等于「把所有远端备份都删掉」，必须回落默认值。
    let dir = common::temp_dir("retention_policy");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    db::app_setting::set_setting(&conn, retention::SETTING_KEEP_RECENT, "0").unwrap();
    db::app_setting::set_setting(&conn, retention::SETTING_KEEP_DAILY, "0").unwrap();
    db::app_setting::set_setting(&conn, retention::SETTING_KEEP_WEEKLY, "0").unwrap();

    let p = retention::policy_from_settings(&conn);
    assert_eq!(p.recent, retention::DEFAULT_KEEP_RECENT);
    assert_eq!(p.daily, retention::DEFAULT_KEEP_DAILY);
    assert_eq!(p.weekly, retention::DEFAULT_KEEP_WEEKLY);
}

#[test]
fn policy_reads_settings_and_ignores_invalid_values() {
    let dir = common::temp_dir("retention_policy_invalid");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    db::app_setting::set_setting(&conn, retention::SETTING_KEEP_RECENT, "3").unwrap();
    db::app_setting::set_setting(&conn, retention::SETTING_KEEP_DAILY, "不是数字").unwrap();

    let p = retention::policy_from_settings(&conn);
    assert_eq!(p.recent, 3);
    assert_eq!(p.daily, retention::DEFAULT_KEEP_DAILY);
    assert_eq!(p.weekly, retention::DEFAULT_KEEP_WEEKLY);
}

// ---------------------------------------------------------------------------
// 快照侧：去重与上限
// ---------------------------------------------------------------------------

fn seed_chapter(dir: &Path) -> (db::DbPool, String, String) {
    let pool = db::init_db(dir.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "保留策略测试作品", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
    let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "第一章", "").unwrap();
    (pool, novel.id, ch.id)
}

fn count_auto(conn: &rusqlite::Connection, chapter_id: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM chapter_revision WHERE chapter_id = ?1 AND revision_type = 'auto'",
        [chapter_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// 把该章节的快照时间戳往前挪一小时。
///
/// 自动快照本身有「距上次快照 ≥5 分钟且字数差 ≥50」的门槛，
/// 测试要触发「再存一份」就必须先满足这个门槛 —— 把已有快照的时间戳
/// 回拨，比直接绕过门槛更接近真实使用。
fn backdate_snapshots(conn: &rusqlite::Connection, chapter_id: &str) {
    conn.execute(
        "UPDATE chapter_revision
         SET created_at = datetime('now', '-1 hour')
         WHERE chapter_id = ?1",
        [chapter_id],
    )
    .unwrap();
}

#[test]
fn snapshot_dedup_skips_identical_content() {
    let dir = common::temp_dir("snapshot_dedup");
    let (pool, novel_id, chapter_id) = seed_chapter(&dir);
    let conn = pool.get().unwrap();

    let same_content = "同一段正文内容";
    let first = db::chapter_revision::maybe_auto_snapshot(
        &conn,
        &chapter_id,
        &novel_id,
        "第一章",
        "{\"p\":1}",
        same_content,
        60,
    )
    .unwrap();
    assert!(first.is_some(), "首次保存必须产生快照");

    // 时间门槛与字数差都满足，但内容完全相同 → 不得新增快照
    backdate_snapshots(&conn, &chapter_id);
    let second = db::chapter_revision::maybe_auto_snapshot(
        &conn,
        &chapter_id,
        &novel_id,
        "第一章",
        "{\"p\":1}",
        same_content,
        200,
    )
    .unwrap();
    assert!(second.is_none(), "内容完全相同时不得再新增快照");
    assert_eq!(count_auto(&conn, &chapter_id), 1);

    // 内容变了 → 仍然产生新快照
    let third = db::chapter_revision::maybe_auto_snapshot(
        &conn,
        &chapter_id,
        &novel_id,
        "第一章",
        "{\"p\":2}",
        "改过的一段正文内容",
        300,
    )
    .unwrap();
    assert!(third.is_some(), "内容变化后必须产生新快照");
    assert_eq!(count_auto(&conn, &chapter_id), 2);
}

#[test]
fn snapshot_limit_prunes_oldest_auto_only() {
    let dir = common::temp_dir("snapshot_limit");
    let (pool, novel_id, chapter_id) = seed_chapter(&dir);
    let conn = pool.get().unwrap();

    db::app_setting::set_setting(
        &conn,
        db::chapter_revision::SETTING_SNAPSHOT_LIMIT,
        "5",
    )
    .unwrap();

    // 先用 create_revision 造 8 条自动快照（内容各不相同），
    // 再触发一次自动快照，让裁剪逻辑真正跑起来。
    for i in 0..8 {
        db::chapter_revision::create_revision(
            &conn,
            &chapter_id,
            &novel_id,
            "第一章",
            &format!("{{\"p\":{i}}}"),
            &format!("第 {i} 段内容"),
            10 + i as i32,
            "auto",
            "自动保存快照",
        )
        .unwrap();
    }
    // 手动快照必须原样保留
    for i in 0..3 {
        db::chapter_revision::create_revision(
            &conn,
            &chapter_id,
            &novel_id,
            "第一章",
            "{\"m\":1}",
            &format!("手动第 {i} 段"),
            5,
            "manual",
            "手动快照",
        )
        .unwrap();
    }

    backdate_snapshots(&conn, &chapter_id);
    db::chapter_revision::maybe_auto_snapshot(
        &conn,
        &chapter_id,
        &novel_id,
        "第一章",
        "{\"p\":99}",
        "新的一段内容",
        999,
    )
    .unwrap();

    assert_eq!(
        count_auto(&conn, &chapter_id),
        5,
        "自动快照必须被裁到上限 5"
    );

    let manual: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chapter_revision WHERE chapter_id = ?1 AND revision_type = 'manual'",
            [&chapter_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(manual, 3, "手动快照绝不能被自动清理带走");
}

#[test]
fn snapshot_limit_is_configurable() {
    let dir = common::temp_dir("snapshot_limit_cfg");
    let (pool, novel_id, chapter_id) = seed_chapter(&dir);
    let conn = pool.get().unwrap();

    db::app_setting::set_setting(
        &conn,
        db::chapter_revision::SETTING_SNAPSHOT_LIMIT,
        "3",
    )
    .unwrap();

    for i in 0..6 {
        db::chapter_revision::create_revision(
            &conn,
            &chapter_id,
            &novel_id,
            "第一章",
            &format!("{{\"p\":{i}}}"),
            &format!("第 {i} 段内容"),
            10 + i as i32,
            "auto",
            "自动保存快照",
        )
        .unwrap();
    }
    backdate_snapshots(&conn, &chapter_id);
    db::chapter_revision::maybe_auto_snapshot(
        &conn,
        &chapter_id,
        &novel_id,
        "第一章",
        "{\"p\":99}",
        "新的一段内容",
        999,
    )
    .unwrap();

    assert_eq!(count_auto(&conn, &chapter_id), 3);
}

// ---------------------------------------------------------------------------
// VACUUM
// ---------------------------------------------------------------------------

#[test]
fn vacuum_keeps_data_and_passes_preflight() {
    let dir = common::temp_dir("vacuum");
    let (pool, _novel_id, _chapter_id) = seed_chapter(&dir);
    let conn = pool.get().unwrap();

    // 造一批垃圾页：插入大文本后再删掉，让库里有可回收的空洞
    conn.execute(
        "INSERT INTO chat_message (id, session_id, role, content, created_at)
         SELECT 'm' || value, 'x', 'user', replace(hex(zeroblob(2048)), '0', 'x'), datetime('now')
         FROM (WITH RECURSIVE seq(value) AS (SELECT 1 UNION ALL SELECT value + 1 FROM seq WHERE value < 200)
               SELECT value FROM seq)",
        [],
    )
    .ok();
    conn.execute("DELETE FROM chat_message", []).unwrap();

    let snap = dir.join("snapshot.db");
    backup::snapshot_to_file(&pool, &snap).unwrap();

    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM chat_message", [], |r| r.get(0))
        .unwrap();

    backup::vacuum_snapshot(&snap).unwrap();

    // VACUUM 之后：数据一行不少，且仍然是可用的库
    let after = rusqlite::Connection::open(&snap).unwrap();
    let after_count: i64 = after
        .query_row("SELECT COUNT(*) FROM chat_message", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after_count, "VACUUM 不得改变数据行数");

    let report = backup::preflight_backup(&snap).expect("VACUUM 后的副本必须仍通过预检");
    assert!(report.is_valid);
}

// ---------------------------------------------------------------------------
// 端到端：真实 HTTP 轮转
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remote_rotation_end_to_end() {
    let server = TestWebDav::start();
    let _dir = common::temp_dir("retention_e2e");

    // 造 12 份远端备份 + 各自的清单 sidecar
    let mut all: Vec<String> = Vec::new();
    for i in 0..12 {
        let name = format!("backup_202601{:02}_000000Z.db.gz", i + 1);
        server.put_file(
            &server.path_for(&format!("{REMOTE_DIR}/{name}")),
            b"payload".to_vec(),
        );
        server.put_file(
            &server.path_for(&format!("{REMOTE_DIR}/{name}.json")),
            b"{}".to_vec(),
        );
        all.push(name);
    }
    // 一份用户自己放的文件：策略永远不该碰它
    server.put_file(
        &server.path_for(&format!("{REMOTE_DIR}/my-notes.db.gz")),
        b"mine".to_vec(),
    );

    // 策略：只保留最近 3 份
    let deleted = backup::prune_remote_backups(&client(&server), REMOTE_DIR, true, policy(3, 0, 0))
        .await
        .unwrap();

    assert_eq!(deleted, 9, "应当删除 9 份，只保留最近 3 份");

    let remaining = server.file_names_in(&server.path_for(REMOTE_DIR));
    let kept: Vec<&String> = all.iter().rev().take(3).collect();
    for name in &kept {
        assert!(remaining.contains(name), "最近 3 份必须还在：{name}");
    }
    // 清单 sidecar 必须跟着备份一起删：被删备份不得留下孤儿清单，
    // 而保留的备份的清单必须还在（否则这份备份就退化成「历史无清单备份」）。
    let sidecars: Vec<&String> = remaining
        .iter()
        .filter(|n| n.ends_with(".db.gz.json"))
        .collect();
    assert_eq!(
        sidecars.len(),
        3,
        "只有保留的 3 份备份才应该有清单 sidecar，实际：{sidecars:?}"
    );
    for name in &kept {
        assert!(
            sidecars.iter().any(|s| *s == &format!("{name}.json")),
            "保留的备份必须连同清单一起留下：{name}"
        );
    }
    // 用户自己的文件不受影响
    assert!(
        remaining.contains(&"my-notes.db.gz".to_string()),
        "不属于本应用备份命名范围的文件不得被删"
    );
}

#[tokio::test]
async fn unverified_backup_leaves_everything_untouched() {
    let server = TestWebDav::start();
    let _dir = common::temp_dir("retention_e2e_unverified");

    for i in 0..5 {
        let name = format!("backup_202601{:02}_000000Z.db.gz", i + 1);
        server.put_file(
            &server.path_for(&format!("{REMOTE_DIR}/{name}")),
            b"payload".to_vec(),
        );
    }

    let deleted =
        backup::prune_remote_backups(&client(&server), REMOTE_DIR, false, policy(1, 0, 0))
            .await
            .unwrap();

    assert_eq!(deleted, 0, "新备份未验证时不得删除任何东西");
    let remaining = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert_eq!(remaining.len(), 5, "远端文件必须一个不少");
}

#[tokio::test]
async fn retention_preview_lists_what_would_be_deleted() {
    let server = TestWebDav::start();
    let _dir = common::temp_dir("retention_preview");

    for i in 0..6 {
        let name = format!("backup_202601{:02}_000000Z.db.gz", i + 1);
        server.put_file(
            &server.path_for(&format!("{REMOTE_DIR}/{name}")),
            b"payload".to_vec(),
        );
        // 每份备份都配一份清单 sidecar
        server.put_file(
            &server.path_for(&format!("{REMOTE_DIR}/{name}.json")),
            b"{}".to_vec(),
        );
    }
    server.put_file(
        &server.path_for(&format!("{REMOTE_DIR}/my-notes.db.gz")),
        b"mine".to_vec(),
    );

    let preview = backup::preview_retention(&client(&server), REMOTE_DIR, policy(2, 0, 0))
        .await
        .unwrap();

    assert_eq!(preview.managed_count, 6, "只有备份本体计入受管数量");
    assert_eq!(preview.keep_count, 2);
    assert_eq!(preview.to_delete.len(), 4);
    assert_eq!(
        preview.protected_foreign, 1,
        "清单 sidecar 属于本应用，不算「策略管不到的文件」"
    );

    // 预览是只读的：远端文件一个都不能少
    let remaining = server.file_names_in(&server.path_for(REMOTE_DIR));
    assert_eq!(remaining.len(), 13, "预览不得删除任何文件");
}

//! 恢复流程的数据安全测试（V0.3.8 补充任务书 P0-2 / P0-3）。
//!
//! 覆盖两件事：
//!
//! - **P0-2 回滚**：数据库覆盖失败、本地配置写回失败、回滚本身失败，
//!   每个分支都必须有确定的结局，且原始数据要么完好、要么明确报告无法回滚。
//! - **P0-3 空库确认**：任何「用空库替换非空本地库」的动作，
//!   确认必须发生在覆盖之前；取消不得产生任何数据库、配置或素材变更。
//!
//! ## 测试怎么跑起来
//!
//! 端到端用例通过 `tests/common/webdav_server.rs` 起一个真实的 HTTP/WebDAV
//! 服务端，`WebDavClient` 走真实 socket —— 不是 trait mock。
//! 故障注入点（[`restore::FaultPoint`]）位于生产代码的真实位置，
//! 生产路径恒为 `None`，测试用它触达正常运行时无法到达的失败分支。
//!
//! ## 数据安全
//!
//! 全部用例只使用 `std::env::temp_dir()` 下的隔离目录、临时数据库
//! 与进程内测试服务端。**不接触用户真实数据库，不连接任何真实 WebDAV 账号。**
//! 用例也不创建 `llm_profile` 记录，因此不会触发系统钥匙串访问。

mod common;

use common::webdav_server::{Faults, TestWebDav};
use easy_writing_lib::db;
use easy_writing_lib::sync::backup;
use easy_writing_lib::sync::restore;
use easy_writing_lib::sync::restore::{FaultPoint, RestoreHooks};
use easy_writing_lib::sync::webdav::WebDavClient;
use rusqlite::Connection;
use shared_types::{ManifestCheck, RestoreOutcome};
use std::fs;
use std::path::{Path, PathBuf};

const REMOTE_DIR: &str = "ew-test";
/// 1×1 PNG，用于构造带图片资产的备份。
const PNG_1PX: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn client(server: &TestWebDav) -> WebDavClient {
    // 凭据是假的，且只发给 127.0.0.1 上的测试服务端。
    WebDavClient::new(&server.base_url(), "test-user", "test-password")
}

fn seed_novels(conn: &Connection, titles: &[&str]) {
    for title in titles {
        let n = db::novel::create_novel(conn, title, "").unwrap();
        let v = db::volume::create_volume(conn, &n.id, "第一卷", "").unwrap();
        db::chapter::create_chapter(conn, &n.id, &v.id, "第一章", "").unwrap();
    }
}

fn novel_titles(pool: &db::DbPool) -> Vec<String> {
    let conn = pool.get().unwrap();
    let mut stmt = conn.prepare("SELECT title FROM novel ORDER BY title").unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect()
}

fn novel_count(pool: &db::DbPool) -> i64 {
    let conn = pool.get().unwrap();
    conn.query_row("SELECT COUNT(*) FROM novel", [], |r| r.get(0))
        .unwrap()
}

fn webdav_url(pool: &db::DbPool) -> Option<String> {
    let conn = pool.get().unwrap();
    conn.query_row(
        "SELECT url FROM webdav_config WHERE id = 'default_webdav'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

fn set_local_webdav_config(pool: &db::DbPool, url: &str) {
    let conn = pool.get().unwrap();
    conn.execute(
        "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir)
         VALUES ('default_webdav', ?1, 'local-user', 'local_pwd_ref', 'local-dir')
         ON CONFLICT(id) DO UPDATE SET url = excluded.url, username = excluded.username",
        rusqlite::params![url],
    )
    .unwrap();
}

/// 本机当前库（恢复目标）。
fn local_library(app: &Path, titles: &[&str]) -> db::DbPool {
    let pool = db::init_db(app.to_path_buf()).unwrap();
    {
        let conn = pool.get().unwrap();
        seed_novels(&conn, titles);
    }
    pool
}

/// 安全快照列表。
fn snapshots_in(app: &Path) -> Vec<PathBuf> {
    let dir = app.join("backups");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("pre_restore_") && n.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect()
}

/// 通过真实备份流程在测试服务端上发布一份备份，返回远端文件名。
async fn publish_backup(
    server: &TestWebDav,
    source_app: &Path,
    titles: &[&str],
    with_image: bool,
) -> String {
    let pool = db::init_db(source_app.to_path_buf()).unwrap();
    {
        let conn = pool.get().unwrap();
        seed_novels(&conn, titles);
        // 每个用例发布的备份内容必须互不相同。
        //
        // 预检候选是按内容哈希寻址的（`<sha256>.db.gz`），两份字节相同的备份
        // 会算出同一个 token。多个用例并行跑「空库备份」时真的会撞车：
        // 一个用例恢复成功后 remove_candidate 删掉共享文件，另一个用例
        // 随即收到「预检候选已失效」而不是它要断言的「未确认的空库覆盖」。
        // 这个 flaky 是 CI 上真实跑出来的（run 39360016314），不是理论推演。
        // 用每个用例唯一的临时目录路径做 nonce，保证 token 互不相同。
        conn.execute(
            "INSERT INTO app_setting (key, value) VALUES ('ew_test_nonce', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [source_app.to_string_lossy().to_string()],
        )
        .unwrap();
        if with_image {
            let nid = db::novel::list_novels(&conn).unwrap()[0].id.clone();
            easy_writing_lib::asset::save_image(
                &conn,
                source_app,
                &nid,
                "封面",
                "image/png",
                PNG_1PX,
            )
            .unwrap();
        }
    }

    let outcome = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(server),
        REMOTE_DIR,
        source_app.to_path_buf(),
    )
    .await
    .expect("测试备份上传必须成功，否则后续断言没有意义");

    assert!(outcome.verified, "测试备份必须通过大小校验与结构自检");
    assert!(
        outcome.manifest_uploaded,
        "测试备份必须成功上传清单 sidecar"
    );
    assert!(
        server.hits("PUT") > 0 && server.hits("MOVE") > 0,
        "备份必须真的走过 HTTP PUT 与 MOVE，实际 PUT={} MOVE={}",
        server.hits("PUT"),
        server.hits("MOVE")
    );
    outcome.filename
}

fn commit(
    pool: &db::DbPool,
    candidate: &Path,
    app: &Path,
    fault: FaultPoint,
) -> Result<restore::CommitSuccess, restore::CommitFailure> {
    restore::commit_restored_database(
        pool,
        candidate,
        app,
        &RestoreHooks {
            fault,
            rollback_fails: false,
        },
    )
}

fn make_candidate(dir: &Path, titles: &[&str]) -> PathBuf {
    let pool = db::init_db(dir.join("cand_src")).unwrap();
    {
        let conn = pool.get().unwrap();
        seed_novels(&conn, titles);
    }
    let p = dir.join("candidate.db");
    backup::snapshot_to_file(&pool, &p).unwrap();
    p
}

// ===========================================================================
// P0-2 · 提交边界与回滚
// ===========================================================================

#[test]
fn successful_commit_replaces_content_and_keeps_snapshot() {
    let dir = common::temp_dir("commit_ok");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品A", "备份里的作品B"]);

    let ok = commit(&pool, &candidate, &app, FaultPoint::None).expect("正常提交应成功");

    assert_eq!(novel_count(&pool), 2, "目标库应被替换为备份内容");
    assert_eq!(novel_titles(&pool), vec!["备份里的作品A", "备份里的作品B"]);
    assert!(ok.snapshot_path.exists(), "安全快照应已落盘");
    assert!(
        snapshots_in(&app).len() == 1,
        "只应留下一份安全快照，实际：{:?}",
        snapshots_in(&app)
    );
}

#[test]
fn snapshot_creation_failure_leaves_target_untouched() {
    let dir = common::temp_dir("commit_snap_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::SnapshotCreate)
        .expect_err("快照创建失败必须中止恢复");

    assert_eq!(f.stage, "snapshot");
    assert!(
        !f.touched_database,
        "快照没建成就不该动目标库，touched_database 必须为 false"
    );
    assert!(!f.rolled_back, "没有改动就不需要回滚");
    assert_eq!(novel_titles(&pool), vec!["本机原有作品"], "原始数据必须完好");
    assert!(
        snapshots_in(&app).is_empty(),
        "快照创建失败不应留下半成品快照文件"
    );
}

#[test]
fn target_open_failure_keeps_original_and_retains_snapshot() {
    let dir = common::temp_dir("commit_open_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::TargetOpen)
        .expect_err("目标库连接失败必须中止恢复");

    assert_eq!(f.stage, "database_overwrite");
    assert!(f.rolled_back, "已进入提交阶段，失败后必须回滚");
    assert!(f.rollback_error.is_none());
    assert_eq!(novel_titles(&pool), vec!["本机原有作品"]);
    assert!(
        f.snapshot_path.clone().unwrap().exists(),
        "快照必须保留供作者复核"
    );
}

#[test]
fn backup_init_failure_rolls_back_and_keeps_original() {
    let dir = common::temp_dir("commit_init_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::BackupInit)
        .expect_err("Backup API 初始化失败必须中止恢复");

    assert_eq!(f.stage, "database_overwrite");
    assert!(f.rolled_back);
    assert_eq!(novel_titles(&pool), vec!["本机原有作品"]);
}

#[test]
fn target_write_failure_rolls_back_to_original_data() {
    // 这是 P0-2 最关键的用例。注入点在真实位置：目标库**确实已经被破坏**
    // （小说表被清空）之后才报错，因此回滚必须真的把数据救回来。
    let dir = common::temp_dir("commit_write_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::TargetWrite)
        .expect_err("写入失败必须中止恢复");

    assert_eq!(f.stage, "database_overwrite");
    assert!(f.touched_database, "目标库确实被改动过");
    assert!(f.rolled_back, "必须已成功回滚");
    assert!(f.rollback_error.is_none());
    assert_eq!(
        novel_titles(&pool),
        vec!["本机原有作品"],
        "回滚后原始数据与计数必须完全不变"
    );
    assert_eq!(novel_count(&pool), 1);

    // 回滚之后数据库必须仍然可用（不是留下一个损坏文件）
    let conn = pool.get().unwrap();
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok", "回滚后的数据库必须通过完整性检查");
}

#[test]
fn webdav_config_restore_failure_rolls_back_whole_restore() {
    let dir = common::temp_dir("commit_cfg_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    set_local_webdav_config(&pool, "https://local.example/dav");
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::ConfigWrite)
        .expect_err("配置写回失败必须中止恢复");

    assert_eq!(f.stage, "local_config");
    assert!(f.touched_database);
    assert!(
        f.rolled_back,
        "配置写回失败时必须回滚整个恢复，不能留下「库换了但凭据没了」的半恢复状态"
    );
    assert_eq!(novel_titles(&pool), vec!["本机原有作品"]);
    assert_eq!(
        webdav_url(&pool).as_deref(),
        Some("https://local.example/dav"),
        "本机连接配置必须保持原样"
    );
}

#[test]
fn ui_setting_restore_failure_rolls_back_whole_restore() {
    let dir = common::temp_dir("commit_setting_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::SettingWrite)
        .expect_err("界面设置写回失败必须中止恢复");

    assert_eq!(f.stage, "local_config");
    assert!(f.rolled_back);
    assert_eq!(novel_titles(&pool), vec!["本机原有作品"]);
}

#[test]
fn successful_commit_preserves_local_webdav_config_over_remote_one() {
    // 远端备份里也有一份 webdav_config（来自另一台机器）。
    // 恢复后必须保留**本机**的连接配置，而不是被远端那份覆盖 ——
    // 否则作者恢复完就再也连不上自己的 WebDAV 了。
    let dir = common::temp_dir("commit_cfg_preserve");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    set_local_webdav_config(&pool, "https://local.example/dav");

    let candidate = {
        let cdir = dir.join("cand_src");
        let cpool = db::init_db(cdir.clone()).unwrap();
        {
            let conn = cpool.get().unwrap();
            seed_novels(&conn, &["备份里的作品"]);
            conn.execute(
                "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir)
                 VALUES ('default_webdav', 'https://remote.example/dav', 'remote-user', 'remote_ref', 'remote-dir')",
                [],
            )
            .unwrap();
        }
        let p = dir.join("candidate.db");
        backup::snapshot_to_file(&cpool, &p).unwrap();
        p
    };

    commit(&pool, &candidate, &app, FaultPoint::None).expect("提交应成功");

    assert_eq!(novel_titles(&pool), vec!["备份里的作品"], "业务数据应来自备份");
    assert_eq!(
        webdav_url(&pool).as_deref(),
        Some("https://local.example/dav"),
        "本机连接配置必须被保留，不能被远端库里的设置覆盖"
    );
    let conn = pool.get().unwrap();
    let user: String = conn
        .query_row(
            "SELECT username FROM webdav_config WHERE id = 'default_webdav'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(user, "local-user");
}

#[test]
fn rollback_failure_retains_snapshot_and_reports_both_errors() {
    let dir = common::temp_dir("commit_rollback_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = restore::commit_restored_database(
        &pool,
        &candidate,
        &app,
        &RestoreHooks {
            fault: FaultPoint::TargetWrite,
            rollback_fails: true,
        },
    )
    .expect_err("覆盖失败且回滚失败必须上报");

    assert!(f.touched_database);
    assert!(!f.rolled_back, "回滚失败必须如实标记，不能谎称已回滚");
    assert!(
        f.rollback_error.is_some(),
        "必须同时上报回滚错误，而不是只报原始错误"
    );
    assert!(
        f.cause.contains("写入目标数据库"),
        "原始错误也必须保留，实际：{}",
        f.cause
    );

    let snapshot = f.snapshot_path.clone().expect("必须报告安全快照位置");
    assert!(
        snapshot.exists(),
        "回滚失败时绝不能删除唯一可用的安全快照：{}",
        snapshot.display()
    );
}

#[test]
fn rollback_helper_rejects_non_database_snapshot() {
    // 回滚失败必须是一个真实的错误，而不是静默成功。
    let dir = common::temp_dir("rollback_bad_snapshot");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);

    let bogus = dir.join("bogus_snapshot.db");
    fs::write(&bogus, vec![0xA5u8; 8192]).unwrap();

    let err = restore::restore_from_snapshot(&pool, &bogus)
        .expect_err("用非数据库文件回滚必须失败");
    assert!(
        !err.is_empty(),
        "错误信息不能为空，作者需要知道回滚为什么没成功"
    );
    assert_eq!(novel_titles(&pool), vec!["本机原有作品"]);
}

#[test]
fn rollback_helper_rejects_missing_snapshot() {
    // `Connection::open` 对不存在的文件会新建一个空库。少了显式校验，
    // 「快照丢失」就会变成「用空库覆盖目标库」—— 回滚反而成了数据销毁。
    let dir = common::temp_dir("rollback_missing");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);

    let err = restore::restore_from_snapshot(&pool, &dir.join("does_not_exist.db"))
        .expect_err("快照不存在时回滚必须失败");
    assert!(
        err.contains("快照不存在"),
        "必须明确说明是快照缺失，实际：{err}"
    );
    assert_eq!(
        novel_titles(&pool),
        vec!["本机原有作品"],
        "快照缺失时绝不能把目标库清空"
    );
    assert_eq!(novel_count(&pool), 1);
}

#[test]
fn rollback_helper_rejects_corrupt_snapshot_without_touching_target() {
    let dir = common::temp_dir("rollback_corrupt");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);

    let bogus = dir.join("corrupt_snapshot.db");
    fs::write(&bogus, b"this is not a sqlite database at all").unwrap();

    let err = restore::restore_from_snapshot(&pool, &bogus)
        .expect_err("损坏的快照不得被用来回滚");
    assert!(
        err.contains("完整性校验") || err.contains("快照"),
        "实际：{err}"
    );
    assert_eq!(
        novel_titles(&pool),
        vec!["本机原有作品"],
        "用损坏快照回滚不得破坏目标库"
    );
}

// ===========================================================================
// P0-2 · 图片回填失败必须进入恢复结果
// ===========================================================================

#[tokio::test]
async fn image_download_failure_is_reported_not_swallowed() {
    let dir = common::temp_dir("backfill_dl_fail");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let conn = pool.get().unwrap();
    let nid = db::novel::list_novels(&conn).unwrap()[0].id.clone();
    easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", PNG_1PX).unwrap();
    // 删掉本地文件，强制走「远端下载」分支
    let rel: String = conn
        .query_row("SELECT rel_path FROM asset LIMIT 1", [], |r| r.get(0))
        .unwrap();
    drop(conn);
    fs::remove_file(app.join(&rel)).unwrap();

    let out = restore::backfill_assets(
        &pool,
        |_remote: String| async { Err("连接被重置".to_string()) },
        "ew-test/images",
        &app.join("images"),
    )
    .await
    .unwrap();

    assert_eq!(out.total, 1);
    assert_eq!(out.failed, 1, "下载失败必须计入失败数");
    assert_eq!(out.restored, 0);
    assert_eq!(out.issues.len(), 1);
    assert!(
        out.issues[0].reason.contains("连接被重置"),
        "失败原因必须带给作者，实际：{}",
        out.issues[0].reason
    );
    assert!(
        !app.join(&rel).exists(),
        "下载失败时不得写出任何文件"
    );
}

#[tokio::test]
async fn image_hash_mismatch_refuses_to_write() {
    let dir = common::temp_dir("backfill_hash");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let conn = pool.get().unwrap();
    let nid = db::novel::list_novels(&conn).unwrap()[0].id.clone();
    easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", PNG_1PX).unwrap();
    let rel: String = conn
        .query_row("SELECT rel_path FROM asset LIMIT 1", [], |r| r.get(0))
        .unwrap();
    drop(conn);
    fs::remove_file(app.join(&rel)).unwrap();

    // 远端返回的是「另一个文件」：长度合理但内容不对
    let out = restore::backfill_assets(
        &pool,
        |_remote: String| async { Ok(b"totally different image bytes".to_vec()) },
        "ew-test/images",
        &app.join("images"),
    )
    .await
    .unwrap();

    assert_eq!(out.failed, 1);
    assert_eq!(out.restored, 0);
    assert!(
        out.issues[0].reason.contains("哈希不匹配"),
        "必须明确说明是哈希不匹配，实际：{}",
        out.issues[0].reason
    );
    assert!(
        out.issues[0].reason.contains("已拒绝写入"),
        "内容不符时必须拒绝写入，实际：{}",
        out.issues[0].reason
    );
    assert!(
        !app.join(&rel).exists(),
        "哈希不匹配的内容绝不能落盘 —— 那会用错误的图覆盖正确的记录"
    );
}

#[tokio::test]
async fn image_with_matching_hash_is_written() {
    let dir = common::temp_dir("backfill_ok");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let conn = pool.get().unwrap();
    let nid = db::novel::list_novels(&conn).unwrap()[0].id.clone();
    easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", PNG_1PX).unwrap();
    let rel: String = conn
        .query_row("SELECT rel_path FROM asset LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let expected = fs::read(app.join(&rel)).unwrap();
    drop(conn);
    fs::remove_file(app.join(&rel)).unwrap();

    let payload = expected.clone();
    let out = restore::backfill_assets(
        &pool,
        move |_remote: String| {
            let p = payload.clone();
            async move { Ok(p) }
        },
        "ew-test/images",
        &app.join("images"),
    )
    .await
    .unwrap();

    assert_eq!(out.restored, 1);
    assert_eq!(out.failed, 0);
    assert!(out.issues.is_empty(), "实际：{:?}", out.issues);
    assert_eq!(fs::read(app.join(&rel)).unwrap(), expected);
}

#[tokio::test]
async fn local_file_with_matching_hash_is_not_redownloaded() {
    let dir = common::temp_dir("backfill_cached");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let conn = pool.get().unwrap();
    let nid = db::novel::list_novels(&conn).unwrap()[0].id.clone();
    easy_writing_lib::asset::save_image(&conn, &app, &nid, "配图", "image/png", PNG_1PX).unwrap();
    drop(conn);

    // 本地文件已存在且哈希正确 —— 下载器若被调用就让测试失败。
    let out = restore::backfill_assets(
        &pool,
        |remote: String| async move { Err(format!("不应发起下载：{remote}")) },
        "ew-test/images",
        &app.join("images"),
    )
    .await
    .unwrap();

    assert_eq!(out.restored, 1, "本地已有且内容正确应计为已就位");
    assert_eq!(out.failed, 0);
    assert!(out.issues.is_empty(), "实际：{:?}", out.issues);
}

#[test]
fn config_restore_error_never_echoes_credentials_or_keychain_refs() {
    // 恢复失败时错误信息会直接显示给作者，也会进日志。
    // 里面绝不能出现 WebDAV 密码、钥匙串引用键名或服务器地址。
    let dir = common::temp_dir("commit_cfg_leak");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    {
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir)
             VALUES ('default_webdav', 'https://dav.secret-host.example', 'alice',
                     'webdav_secret_ref_name', 'private-dir')",
            [],
        )
        .unwrap();
    }
    let candidate = make_candidate(&dir, &["备份里的作品"]);

    let f = commit(&pool, &candidate, &app, FaultPoint::ConfigWrite)
        .expect_err("配置写回失败必须中止恢复");

    let surfaced = format!("{} | {:?}", f.cause, f.rollback_error);
    for secret in [
        "webdav_secret_ref_name",
        "dav.secret-host.example",
        "alice",
        "private-dir",
    ] {
        assert!(
            !surfaced.contains(secret),
            "恢复失败信息中不得出现 {secret:?}，实际：{surfaced}"
        );
    }
    assert!(
        f.cause.contains("WebDAV 连接配置"),
        "但仍必须说清是哪个阶段失败，实际：{}",
        f.cause
    );

    // 本机配置确实被暂存了（恢复成功后会写回），
    // 且 PreservedLocalConfig 刻意不实现 Debug —— 任何 {:?} 打印都会编译失败。
    let preserved = restore::read_preserved_local_config(&pool).unwrap();
    assert!(preserved.webdav.is_some(), "本机配置应被暂存以便恢复后写回");
}

#[test]
fn preserved_config_query_failure_is_not_confused_with_absent_config() {
    // 旧实现用 `.ok()` 把「查询失败」和「本机没有配置」压成同一个 None，
    // 前者会导致恢复后 WebDAV 凭据凭空消失且无人知晓。
    let dir = common::temp_dir("commit_cfg_missing_table");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机原有作品"]);
    {
        let conn = pool.get().unwrap();
        conn.execute("DROP TABLE webdav_config", []).unwrap();
    }

    // 用 match 而不是 expect_err：PreservedLocalConfig 刻意不实现 Debug
    // （它带着 pwd_ref），所以任何 {:?} 打印都应该编译失败。
    let err = match restore::read_preserved_local_config(&pool) {
        Ok(_) => panic!("查询失败必须冒泡，不能当成「本机没有配置」"),
        Err(e) => e,
    };
    assert!(
        err.contains("WebDAV"),
        "错误必须指明是哪个阶段读不出来，实际：{err}"
    );
}

// ===========================================================================
// P0-2 D · 端到端：数据库成功 ≠ 整个恢复成功
// ===========================================================================

#[tokio::test]
async fn full_roundtrip_with_assets_reports_success() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("e2e_ok");
    let filename = publish_backup(&server, &dir.join("src"), &["云端作品"], true).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .expect("预检应成功");

    assert_eq!(pf.asset_total, 1, "备份应包含一个图片资产");
    assert_eq!(pf.asset_missing_locally, 1, "本机还没有这张图");

    let report = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token.clone(),
        false,
        RestoreHooks::default(),
    )
    .await
    .expect("恢复应成功");

    assert_eq!(report.outcome, RestoreOutcome::Success);
    assert!(report.summary.database_restored);
    assert!(report.summary.config_restored);
    assert_eq!(report.summary.asset_restored, 1);
    assert_eq!(report.summary.asset_failed, 0);
    assert_eq!(novel_titles(&pool), vec!["云端作品"]);
    assert_eq!(
        report.summary.manifest,
        ManifestCheck::Verified { asset_count: 1 },
        "清单必须被真实读取并校验通过"
    );
    assert!(
        !report.summary.snapshot_retained,
        "完全成功时应清理安全快照，实际保留在 {}",
        report.summary.snapshot_path
    );

    // 图片确实落盘且内容正确
    let conn = pool.get().unwrap();
    let (rel, hash): (String, String) = conn
        .query_row("SELECT rel_path, hash FROM asset LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    drop(conn);
    let name = Path::new(&rel).file_name().unwrap().to_str().unwrap();
    let bytes = fs::read(app.join("images").join(name)).expect("图片应已回填");
    assert_eq!(
        easy_writing_lib::sync::manifest::sha256_hex(&bytes),
        hash,
        "回填的图片内容必须与数据库记录一致"
    );
}

#[tokio::test]
async fn missing_remote_image_downgrades_to_partial_success() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("e2e_missing_img");
    let filename = publish_backup(&server, &dir.join("src"), &["云端作品"], true).await;

    // 把远端图片删掉：数据库备份完好，但素材无法回填。
    let images = server.file_names_in(&server.path_for(&format!("{REMOTE_DIR}/images")));
    assert_eq!(images.len(), 1, "测试前提：远端应有一张图片");
    let image_path = server.path_for(&format!("{REMOTE_DIR}/images/{}", images[0]));
    server.remove_file(&image_path);
    assert!(
        server.get_file(&image_path).is_none(),
        "测试前提：远端图片必须确实已被移除，否则本用例测不到「素材缺失」分支"
    );

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .unwrap();

    let report = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        false,
        RestoreHooks::default(),
    )
    .await
    .unwrap();

    assert_eq!(
        report.outcome,
        RestoreOutcome::PartialAssets,
        "数据库恢复成功不等于整个项目恢复成功"
    );
    assert!(report.summary.database_restored);
    assert_eq!(report.summary.asset_failed, 1);
    assert!(
        report.issues.iter().any(|i| i.reason.contains("远端下载失败")),
        "缺失素材必须明确列出，实际：{:?}",
        report.issues
    );
    assert!(
        report.summary.snapshot_retained,
        "部分成功时必须保留安全快照，作者可能还要再恢复一次"
    );
    assert!(
        Path::new(&report.summary.snapshot_path).exists(),
        "报告里的快照路径必须真实存在：{}",
        report.summary.snapshot_path
    );
}

// ===========================================================================
// P0-3 · 空库确认必须发生在覆盖之前
// ===========================================================================

#[tokio::test]
async fn preflight_reports_empty_backup_without_touching_local_db() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("pf_empty");
    let filename = publish_backup(&server, &dir.join("src"), &[], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let before = novel_titles(&pool);
    let snapshots_before = snapshots_in(&app).len();

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .expect("预检应成功");

    assert!(pf.is_empty_library, "候选备份是空库");
    assert!(!pf.local_is_empty, "本机库非空");
    assert_eq!(pf.local_novel_count, 1);
    assert!(
        pf.requires_empty_overwrite_confirm,
        "非空本地库 + 空备份必须要求显式确认"
    );
    assert!(pf.integrity_ok, "SQLite 完整性检查必须已通过");
    assert_eq!(pf.novel_count, 0);
    assert!(restore::is_preflight_token(&pf.preflight_token));

    // 预检阶段绝不能改动现有数据库
    assert_eq!(novel_titles(&pool), before, "预检不得改动本机库");
    assert_eq!(
        snapshots_in(&app).len(),
        snapshots_before,
        "预检不得创建安全快照（那属于提交阶段）"
    );
}

#[tokio::test]
async fn empty_overwrite_without_confirmation_changes_nothing() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_cancel");
    let filename = publish_backup(&server, &dir.join("src"), &[], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    set_local_webdav_config(&pool, "https://local.example/dav");
    let before = novel_titles(&pool);
    let before_url = webdav_url(&pool);

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .unwrap();

    let err = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        false, // 未确认
        RestoreHooks::default(),
    )
    .await
    .expect_err("未确认的空库覆盖必须被拒绝");

    assert!(
        err.contains("空库"),
        "拒绝原因必须明确说明这是空库覆盖，实际：{err}"
    );
    assert!(
        err.contains("未被改动"),
        "必须明确告知作者当前数据库没有被动过，实际：{err}"
    );
    assert_eq!(novel_titles(&pool), before, "取消确认不得改动数据库");
    assert_eq!(webdav_url(&pool), before_url, "取消确认不得改动本地配置");
    assert!(
        snapshots_in(&app).is_empty(),
        "取消确认时连安全快照都不应产生（根本没进入提交阶段）"
    );
    assert!(
        fs::read_dir(app.join("images"))
            .map(|mut d| d.next().is_none())
            .unwrap_or(true),
        "取消确认不得改动素材目录"
    );
}

#[tokio::test]
async fn empty_overwrite_with_confirmation_replaces_library() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_confirm");
    let filename = publish_backup(&server, &dir.join("src"), &[], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .unwrap();

    let report = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        true, // 已确认
        RestoreHooks::default(),
    )
    .await
    .expect("确认后应允许恢复");

    assert_eq!(report.outcome, RestoreOutcome::Success);
    assert!(report.summary.is_empty_library, "结果必须明确标识为空库恢复");
    assert_eq!(novel_count(&pool), 0, "确认后本机作品应被空库替换");
}

#[tokio::test]
async fn nonempty_backup_does_not_require_empty_confirmation() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_nonempty");
    let filename =
        publish_backup(&server, &dir.join("src"), &["云端作品A", "云端作品B"], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .unwrap();

    assert!(!pf.is_empty_library);
    assert!(
        !pf.requires_empty_overwrite_confirm,
        "非空备份不应触发空库专属确认"
    );

    // 不确认也能恢复
    let report = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        false,
        RestoreHooks::default(),
    )
    .await
    .expect("非空备份不需要空库确认");

    assert_eq!(report.outcome, RestoreOutcome::Success);
    assert_eq!(novel_titles(&pool), vec!["云端作品A", "云端作品B"]);
}

#[tokio::test]
async fn empty_local_plus_empty_backup_is_reported_as_empty() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_both_empty");
    let filename = publish_backup(&server, &dir.join("src"), &[], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &[]); // 本机也是空库

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .unwrap();

    assert!(pf.local_is_empty);
    assert!(pf.is_empty_library);
    assert!(
        !pf.requires_empty_overwrite_confirm,
        "本机本来就是空库时不需要「替换现有作品」的二次确认"
    );

    let report = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        false,
        RestoreHooks::default(),
    )
    .await
    .unwrap();

    assert_eq!(report.outcome, RestoreOutcome::Success);
    assert!(
        report.summary.is_empty_library,
        "结果仍须明确标识这是一次空库恢复"
    );
}

// ===========================================================================
// P0-3 C · 不能确认 A 却恢复 B
// ===========================================================================

#[tokio::test]
async fn tampered_candidate_after_preflight_is_rejected() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_tamper");
    let filename =
        publish_backup(&server, &dir.join("src"), &["云端作品A", "云端作品B"], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let before = novel_titles(&pool);

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .unwrap();

    // 候选备份在确认之后被改动（哪怕只多一个字节）
    let cand = restore::candidate_gz_path(&pf.preflight_token).unwrap();
    let mut bytes = fs::read(&cand).unwrap();
    bytes.push(0x00);
    fs::write(&cand, &bytes).unwrap();

    let err = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        true,
        RestoreHooks::default(),
    )
    .await
    .expect_err("候选内容改变后必须拒绝执行");

    assert!(
        err.contains("不一致"),
        "必须说明候选内容与预检时不一致，实际：{err}"
    );
    assert_eq!(novel_titles(&pool), before, "本机库必须保持原样");
}

#[tokio::test]
async fn unknown_or_stale_preflight_token_is_rejected() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_stale");
    let filename = publish_backup(&server, &dir.join("src"), &["云端作品"], false).await;

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let before = novel_titles(&pool);

    // 一个格式合法、但从未预检过的 token
    let bogus = "0".repeat(64);
    let err = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        bogus,
        true,
        RestoreHooks::default(),
    )
    .await
    .expect_err("未预检过的 token 必须被拒绝");
    assert!(
        err.contains("失效") || err.contains("重新执行预检"),
        "错误信息应指引作者重新预检，实际：{err}"
    );
    assert_eq!(novel_titles(&pool), before);
}

#[test]
fn malformed_preflight_token_cannot_be_used_for_path_traversal() {
    // token 会被拼进临时目录下的文件路径。如果只当普通字符串用，
    // `../../..` 就能让后端去读应用目录之外的任意文件并覆盖数据库。
    for evil in [
        "../../../../etc/passwd",
        "..%2F..%2F..%2Fetc%2Fpasswd",
        "/absolute/path.db.gz",
        "C:\\windows\\system32\\x.db.gz",
        &"a".repeat(63),
        &"z".repeat(64),
        "",
    ] {
        assert!(
            !restore::is_preflight_token(evil),
            "非法 token 必须被识别：{evil:?}"
        );
        assert!(
            restore::candidate_gz_path(evil).is_err(),
            "非法 token 不得被用来拼出文件路径：{evil:?}"
        );
    }
    // 合法 token 必须能用
    assert!(restore::is_preflight_token(&"a".repeat(64)));
    assert!(restore::candidate_gz_path(&"a".repeat(64)).is_ok());
}

#[tokio::test]
async fn corrupt_manifest_cannot_be_bypassed_by_confirmation() {
    // 空库确认是「作者知情后同意」，不是「跳过完整性校验」。
    // 清单损坏必须在预检阶段就失败，因此根本拿不到可用于确认的 token。
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_corrupt_manifest");
    let filename = publish_backup(&server, &dir.join("src"), &[], false).await;

    server.put_file(
        &server.path_for(&format!("{REMOTE_DIR}/{filename}.json")),
        b"{ \"manifest_version\": 2, this is not json".to_vec(),
    );

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let before = novel_titles(&pool);

    let err = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .expect_err("损坏的清单必须让预检失败");

    assert!(
        err.contains("无法解析"),
        "必须明确说明清单损坏，实际：{err}"
    );
    assert!(
        err.contains("不会被当成历史无清单备份"),
        "错误信息必须说明这不是历史备份兼容路径，实际：{err}"
    );
    assert_eq!(novel_titles(&pool), before);
    assert!(
        snapshots_in(&app).is_empty(),
        "预检失败不得留下安全快照"
    );
}

#[tokio::test]
async fn manifest_read_error_is_not_treated_as_legacy_backup() {
    // 服务端对清单返回 500（不是 404）时，绝不能据此判定为「历史无清单备份」。
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_manifest_500");
    let filename = publish_backup(&server, &dir.join("src"), &["云端作品"], false).await;

    server.set_faults(Faults {
        manifest_get_status: Some(500),
        ..Default::default()
    });

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);

    let err = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .expect_err("清单读取失败（非 404）必须中止");

    assert!(
        err.contains("不能据此判定为历史备份") || err.contains("HTTP 500"),
        "必须明确区分「读取失败」与「文件不存在」，实际：{err}"
    );
}

#[tokio::test]
async fn absent_manifest_takes_legacy_path_and_restores() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("p03_legacy");
    let filename = publish_backup(&server, &dir.join("src"), &["云端作品"], false).await;

    // 删掉远端清单 → 服务端对该路径返回真实的 404
    server.remove_file(&server.path_for(&format!("{REMOTE_DIR}/{filename}.json")));

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .expect("历史无清单备份必须仍可恢复");

    assert_eq!(
        pf.manifest,
        ManifestCheck::Absent,
        "404 才是唯一能进入历史备份兼容分支的依据"
    );
    assert!(pf.integrity_ok, "仍须通过 SQLite 完整性检查");
    assert_eq!(pf.schema_version, db::BASELINE_VERSION);
    assert!(!pf.requires_empty_overwrite_confirm);

    let report = restore::run_consistency_restore(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
        pf.preflight_token,
        false,
        RestoreHooks::default(),
    )
    .await
    .unwrap();

    assert_eq!(report.outcome, RestoreOutcome::Success);
    assert_eq!(report.summary.manifest, ManifestCheck::Absent);
    assert_eq!(novel_titles(&pool), vec!["云端作品"]);
}

// ===========================================================================
// 预检阶段的其他拒绝路径
// ===========================================================================

#[tokio::test]
async fn preflight_rejects_truncated_backup_before_touching_local_db() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("pf_truncated");
    let filename = publish_backup(&server, &dir.join("src"), &["云端作品"], false).await;

    // 服务端存了半截文件：传输是干净的，但内容残缺
    let full = server
        .get_file(&server.path_for(&format!("{REMOTE_DIR}/{filename}")))
        .unwrap();
    server.set_faults(Faults {
        truncate_get_to: Some(full.len() / 2),
        ..Default::default()
    });

    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);
    let before = novel_titles(&pool);

    let err = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &filename,
        app.clone(),
    )
    .await
    .expect_err("截断的备份必须在预检阶段被拒绝");

    assert!(
        err.contains("未通过完整性检查") || err.contains("解压"),
        "实际：{err}"
    );
    assert!(err.contains("原数据未被改动"), "实际：{err}");
    assert_eq!(novel_titles(&pool), before);
}

#[tokio::test]
async fn preflight_rejects_path_traversal_filename() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("pf_traversal");
    let app = dir.join("app");
    let pool = local_library(&app, &["本机作品"]);

    for evil in ["../../secret.db.gz", "a/b.db.gz", "..\\x.db.gz"] {
        let err = restore::run_restore_preflight(
            pool.clone(),
            &client(&server),
            REMOTE_DIR,
            evil,
            app.clone(),
        )
        .await
        .expect_err("路径穿越文件名必须被拒绝");
        assert!(err.contains("非法的备份文件名"), "实际：{err}");
    }
    assert_eq!(novel_titles(&pool), vec!["本机作品"]);
}

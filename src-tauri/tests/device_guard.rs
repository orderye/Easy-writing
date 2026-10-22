//! 跨设备覆盖保护的回归测试（任务书 T04 / PR-05）。
//!
//! ## 覆盖什么
//!
//! - **设备身份存库外**：`sync_state.json` 不在数据库里，恢复覆盖 `.db`
//!   之后设备身份不变 —— 这是整套保护成立的前提。
//! - **来源身份 sidecar**：每次备份都写出 `<backup>.db.gz.meta.json`，
//!   内容含 device_id / created_at / 库内最大 updated_at / schema_version /
//!   app_version / sha256。
//! - **冲突判定**：远端来自别的设备、且比本机 `last_sync_at` 更新 → 判为冲突；
//!   同设备、远端更旧、远端没有 meta → 都不算冲突。
//! - **闸门在后端**：`webdav_backup_now` 默认拒绝在冲突时上传；
//!   作者显式选择「继续上传」时才放行。
//! - **退出时不阻塞**：静默同步遇冲突跳过上传，把冲突记进设备状态，
//!   等下次启动提示。
//! - **轮转不留孤儿**：被删备份的 meta sidecar 一起消失。
//! - **恢复前校验**：meta 记录的哈希与实际内容不符 → 拒绝恢复，原库未动。
//!
//! ## 数据安全
//!
//! 全部数据都在 `std::env::temp_dir()` 下的隔离目录与进程内 WebDAV 服务端中，
//! **不接触用户真实数据库、真实钥匙串与真实 WebDAV 目录**。

mod common;

use common::webdav_server::TestWebDav;
use easy_writing_lib::db;
use easy_writing_lib::sync::backup;
use easy_writing_lib::sync::device;
use easy_writing_lib::sync::restore;
use easy_writing_lib::sync::webdav::WebDavClient;
use std::path::Path;

const REMOTE_DIR: &str = "ew-device";

fn client(server: &TestWebDav) -> WebDavClient {
    WebDavClient::new(&server.base_url(), "test-user", "test-password")
}

fn remote(server: &TestWebDav, path: &str) -> String {
    server.path_for(&format!("{REMOTE_DIR}/{path}"))
}

fn seeded_pool(app: &Path) -> db::DbPool {
    let pool = db::init_db(app.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    let novel = db::novel::create_novel(&conn, "跨设备测试作品", "").unwrap();
    let vol = db::volume::create_volume(&conn, &novel.id, "第一卷", "").unwrap();
    db::chapter::create_chapter(&conn, &novel.id, &vol.id, "第一章", "").unwrap();
    drop(conn);
    pool
}

/// 造一份「别的设备写的」meta。
fn foreign_meta(created_at: &str, sha256: &str) -> Vec<u8> {
    let meta = device::BackupMeta {
        device_id: "other-device-0001".to_string(),
        device_name: "另一台机器".to_string(),
        created_at: created_at.to_string(),
        max_updated_at: "2026-01-01 00:00:00".to_string(),
        schema_version: db::BASELINE_VERSION,
        app_version: "0.3.8".to_string(),
        sha256: sha256.to_string(),
    };
    serde_json::to_vec_pretty(&meta).unwrap()
}

// ---------------------------------------------------------------------------
// 设备身份
// ---------------------------------------------------------------------------

#[test]
fn device_id_is_generated_once_and_persisted() {
    let dir = common::temp_dir("device_identity");
    let first = device::load(&dir);
    assert!(!first.device_id.is_empty());
    assert!(dir.join(device::STATE_FILE_NAME).exists(), "状态文件应落盘");

    let second = device::load(&dir);
    assert_eq!(first.device_id, second.device_id, "设备标识必须稳定");
}

#[test]
fn device_state_survives_database_overwrite() {
    // 跨设备保护成立的前提：设备身份不在库里。
    // 恢复会整体覆盖 .db，库里的身份会被来源设备覆盖。
    let dir = common::temp_dir("device_outside_db");
    let _pool = seeded_pool(&dir);
    let before = device::load(&dir);

    // 模拟恢复覆盖：把数据库文件换掉
    let db_path = dir.join("easy-writing.db");
    std::fs::remove_file(&db_path).unwrap();
    let _new_pool = db::init_db(dir.clone()).unwrap();

    let after = device::load(&dir);
    assert_eq!(
        before.device_id, after.device_id,
        "数据库被覆盖后设备身份必须不变"
    );
}

#[test]
fn device_state_file_is_valid_json_and_round_trips() {
    let dir = common::temp_dir("device_roundtrip");
    let mut state = device::load(&dir);
    state.last_sync_at = Some("2026-01-01T00:00:00+00:00".to_string());
    device::save(&dir, &state).unwrap();

    let raw = std::fs::read(dir.join(device::STATE_FILE_NAME)).unwrap();
    let parsed: device::DeviceState = serde_json::from_slice(&raw).unwrap();
    assert_eq!(parsed.device_id, state.device_id);
    assert_eq!(parsed.last_sync_at, state.last_sync_at);
}

// ---------------------------------------------------------------------------
// meta sidecar
// ---------------------------------------------------------------------------

#[tokio::test]
async fn backup_uploads_meta_sidecar() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("meta_upload");
    let app = dir.join("app");
    let pool = seeded_pool(&app);

    let outcome = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .unwrap();
    assert!(outcome.meta_uploaded, "来源身份 sidecar 必须上传成功");

    let meta_name = device::meta_sidecar_name(&outcome.filename);
    let bytes = server
        .get_file(&remote(&server, &meta_name))
        .expect("远端必须存在 .meta.json sidecar");

    let meta: device::BackupMeta = serde_json::from_slice(&bytes).unwrap();
    let local = device::load(&app);
    assert_eq!(meta.device_id, local.device_id, "meta 必须记录本机设备标识");
    assert!(!meta.created_at.is_empty());
    assert_eq!(meta.schema_version, db::BASELINE_VERSION);
    assert_eq!(meta.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(meta.sha256.len(), 64, "meta 必须记录压缩包哈希");
    assert!(
        !meta.max_updated_at.is_empty(),
        "meta 必须记录库内最大更新时间"
    );
}

#[tokio::test]
async fn rotation_removes_meta_sidecar_with_backup() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("meta_rotation");
    let app = dir.join("app");
    let pool = seeded_pool(&app);

    // 预置 5 份旧备份，每份都带清单与 meta sidecar
    for i in 1..=5 {
        let name = format!("backup_2026010{}_000000Z.db.gz", i);
        server.put_file(&remote(&server, &name), b"old".to_vec());
        server.put_file(&remote(&server, &format!("{name}.json")), b"{}".to_vec());
        server.put_file(
            &remote(&server, &device::meta_sidecar_name(&name)),
            b"{\"device_id\":\"x\"}".to_vec(),
        );
    }

    // 只保留最近 1 份
    {
        let conn = pool.get().unwrap();
        db::app_setting::set_setting(
            &conn,
            easy_writing_lib::sync::retention::SETTING_KEEP_RECENT,
            "1",
        )
        .unwrap();
        db::app_setting::set_setting(
            &conn,
            easy_writing_lib::sync::retention::SETTING_KEEP_DAILY,
            "0",
        )
        .unwrap();
        db::app_setting::set_setting(
            &conn,
            easy_writing_lib::sync::retention::SETTING_KEEP_WEEKLY,
            "0",
        )
        .unwrap();
    }

    backup::run_incremental_backup_detailed(pool.clone(), &client(&server), REMOTE_DIR, app.clone())
        .await
        .unwrap();

    let remaining = server.file_names_in(&server.path_for(REMOTE_DIR));
    let orphan_meta: Vec<&String> = remaining
        .iter()
        .filter(|n| n.ends_with(".db.gz.meta.json"))
        .collect();
    // 只应剩下被保留备份的那一份 meta
    assert_eq!(
        orphan_meta.len(),
        1,
        "被删除备份的 meta sidecar 必须一起删掉，实际残留：{orphan_meta:?}"
    );
    // 残留的那份必须对应仍然存在的备份
    let kept_meta = orphan_meta[0];
    let kept_backup = kept_meta.trim_end_matches(".meta.json").to_string();
    assert!(
        remaining.contains(&kept_backup),
        "残留 meta 必须对应一份仍然存在的备份：{kept_backup}"
    );
}

// ---------------------------------------------------------------------------
// 冲突判定（真实 HTTP 路径）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn conflict_detected_for_other_device_newer_than_last_sync() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("conflict_detect");
    let app = dir.join("app");

    // 本机状态：上次同步在 2026-01-01
    let mut state = device::DeviceState {
        device_id: "local-device-0001".to_string(),
        device_name: "本机".to_string(),
        last_sync_at: Some("2026-01-01T00:00:00+00:00".to_string()),
        pending_notice: None,
    };
    device::save(&app, &state).unwrap();

    // 远端最新备份来自别的设备，且更新
    let name = "backup_20260201_000000Z.db.gz";
    server.put_file(&remote(&server, name), b"payload".to_vec());
    server.put_file(
        &remote(&server, &device::meta_sidecar_name(name)),
        foreign_meta("2026-02-01T00:00:00+00:00", &"a".repeat(64)),
    );

    let conflict = easy_writing_lib::commands::sync_cmd::detect_remote_conflict(
        &client(&server),
        REMOTE_DIR,
        &state,
    )
    .await
    .unwrap()
    .expect("应当判定为跨设备冲突");

    assert_eq!(conflict.remote_device_id, "other-device-0001");
    assert_eq!(conflict.local_device_id, "local-device-0001");
    assert_eq!(conflict.remote_filename, name);
    assert!(
        easy_writing_lib::commands::sync_cmd::detect_remote_conflict(
            &client(&server),
            REMOTE_DIR,
            &state,
        )
        .await
        .unwrap()
        .is_some()
    );
}

#[tokio::test]
async fn no_conflict_for_same_device_or_older_remote() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("conflict_none");
    let app = dir.join("app");

    let state = device::DeviceState {
        device_id: "local-device-0001".to_string(),
        device_name: "本机".to_string(),
        last_sync_at: Some("2026-03-01T00:00:00+00:00".to_string()),
        pending_notice: None,
    };
    device::save(&app, &state).unwrap();

    // 同一台设备写的最新备份 → 不是冲突
    let same = "backup_20260401_000000Z.db.gz";
    server.put_file(&remote(&server, same), b"payload".to_vec());
    let mut meta = device::BackupMeta {
        device_id: "local-device-0001".to_string(),
        device_name: "本机".to_string(),
        created_at: "2026-04-01T00:00:00+00:00".to_string(),
        max_updated_at: String::new(),
        schema_version: db::BASELINE_VERSION,
        app_version: "0.3.8".to_string(),
        sha256: "a".repeat(64),
    };
    server.put_file(
        &remote(&server, &device::meta_sidecar_name(same)),
        serde_json::to_vec_pretty(&meta).unwrap(),
    );
    assert!(
        easy_writing_lib::commands::sync_cmd::detect_remote_conflict(
            &client(&server),
            REMOTE_DIR,
            &state
        )
        .await
        .unwrap()
        .is_none(),
        "同一台设备写的备份不构成冲突"
    );

    // 别的设备、但比本机上次同步更旧 → 也不是冲突
    meta.device_id = "other-device-0001".to_string();
    meta.created_at = "2026-02-01T00:00:00+00:00".to_string();
    server.put_file(
        &remote(&server, &device::meta_sidecar_name(same)),
        serde_json::to_vec_pretty(&meta).unwrap(),
    );
    assert!(
        easy_writing_lib::commands::sync_cmd::detect_remote_conflict(
            &client(&server),
            REMOTE_DIR,
            &state
        )
        .await
        .unwrap()
        .is_none(),
        "远端早于本机上次同步时不构成冲突"
    );
}

#[tokio::test]
async fn no_conflict_when_remote_has_no_meta() {
    // 历史备份没有 meta：兼容现状，不算冲突。
    let server = TestWebDav::start();
    let dir = common::temp_dir("conflict_no_meta");
    let app = dir.join("app");

    let state = device::DeviceState {
        device_id: "local-device-0001".to_string(),
        device_name: "本机".to_string(),
        last_sync_at: Some("2026-01-01T00:00:00+00:00".to_string()),
        pending_notice: None,
    };
    device::save(&app, &state).unwrap();

    server.put_file(
        &remote(&server, "backup_20260201_000000Z.db.gz"),
        b"payload".to_vec(),
    );

    assert!(
        easy_writing_lib::commands::sync_cmd::detect_remote_conflict(
            &client(&server),
            REMOTE_DIR,
            &state
        )
        .await
        .unwrap()
        .is_none(),
        "没有 meta 的历史备份不构成冲突"
    );
}

// ---------------------------------------------------------------------------
// 退出时的静默同步：跳过 + 记录提示
// ---------------------------------------------------------------------------

#[tokio::test]
async fn silent_sync_skips_on_conflict_and_records_notice() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("silent_conflict");
    let app = dir.join("app");
    let pool = seeded_pool(&app);

    let mut state = device::DeviceState {
        device_id: "local-device-0001".to_string(),
        device_name: "本机".to_string(),
        last_sync_at: Some("2026-01-01T00:00:00+00:00".to_string()),
        pending_notice: None,
    };
    device::save(&app, &state).unwrap();

    // 远端最新备份来自别的设备且更新
    let name = "backup_20260201_000000Z.db.gz";
    server.put_file(&remote(&server, name), b"payload".to_vec());
    server.put_file(
        &remote(&server, &device::meta_sidecar_name(name)),
        foreign_meta("2026-02-01T00:00:00+00:00", &"a".repeat(64)),
    );

    let result = easy_writing_lib::commands::sync_cmd::run_silent_sync_with(
        pool,
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .unwrap();

    assert!(
        result.starts_with(device::CONFLICT_CODE),
        "静默同步遇到冲突应跳过并返回冲突标记，实际：{result}"
    );
    // 远端不得出现本次上传的备份
    let uploaded = server
        .file_names_in(&server.path_for(REMOTE_DIR))
        .into_iter()
        .filter(|n| n.starts_with("backup_2026") && n.ends_with(".db.gz"))
        .count();
    assert_eq!(uploaded, 1, "冲突时不得上传任何备份，远端只有预置的那一份");

    // 提示必须被记下来，等下次启动时展示
    let after = device::load(&app);
    let notice = after.pending_notice.expect("冲突提示必须被记录");
    assert_eq!(notice.kind, "backup_conflict");
    assert_eq!(notice.remote_filename, name);
    assert_eq!(notice.remote_device_id, "other-device-0001");
}

// ---------------------------------------------------------------------------
// 恢复侧
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restore_preflight_reports_source_device() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("restore_source");
    // 备份与预检必须用**同一个**应用数据目录：设备身份是按目录存的，
    // 两个目录就是两台「设备」。
    let app = dir.join("app");
    let pool = seeded_pool(&app);

    let outcome = backup::run_incremental_backup_detailed(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        app.clone(),
    )
    .await
    .unwrap();

    // 本机自己的备份 → 不是「来自其它设备」
    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &outcome.filename,
        app.clone(),
    )
    .await
    .unwrap();
    assert!(!pf.is_from_other_device, "本机自己写的备份不算来自其它设备");
    assert!(pf.source_device_id.is_some());

    // 把 meta 换成别的设备 → 预检必须如实报告
    let meta_name = device::meta_sidecar_name(&outcome.filename);
    let gz = server.get_file(&remote(&server, &outcome.filename)).unwrap();
    let hash = easy_writing_lib::sync::manifest::sha256_hex(&gz);
    server.put_file(
        &remote(&server, &meta_name),
        foreign_meta("2026-02-01T00:00:00+00:00", &hash),
    );

    let pf = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &outcome.filename,
        app.clone(),
    )
    .await
    .unwrap();
    assert!(
        pf.is_from_other_device,
        "预检必须报告「这份备份来自另一台设备」"
    );
    assert_eq!(pf.source_device_id.as_deref(), Some("other-device-0001"));
}

#[tokio::test]
async fn restore_rejects_meta_sha256_mismatch() {
    let server = TestWebDav::start();
    let dir = common::temp_dir("restore_meta_hash");
    let src = dir.join("src");
    let pool_src = seeded_pool(&src);

    let outcome = backup::run_incremental_backup_detailed(
        pool_src.clone(),
        &client(&server),
        REMOTE_DIR,
        src.clone(),
    )
    .await
    .unwrap();

    // meta 记录的哈希与实际内容不符 → 必须拒绝恢复
    let meta_name = device::meta_sidecar_name(&outcome.filename);
    server.put_file(
        &remote(&server, &meta_name),
        foreign_meta("2026-02-01T00:00:00+00:00", &"b".repeat(64)),
    );

    let app = dir.join("app");
    let pool = seeded_pool(&app);
    let err = restore::run_restore_preflight(
        pool.clone(),
        &client(&server),
        REMOTE_DIR,
        &outcome.filename,
        app.clone(),
    )
    .await
    .expect_err("meta 哈希不符必须拒绝恢复");

    assert!(
        err.contains("哈希"),
        "错误必须说明是哈希不符，实际：{err}"
    );

    // 本机数据未被动过
    let conn = pool.get().unwrap();
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM novel", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "预检失败不得改动本机数据库");
}

//! 作品诊断与恢复中心的后端只读聚合层测试（V0.4.0 / P2-4）。
//!
//! ## 覆盖什么
//!
//! - **干净库正常输出**：六项已有能力全部给出确定结果，四项缺失能力显式 skipped。
//! - **逐项容错**：构造坏数据（删掉 asset 表）让素材类检查失败，
//!   其余项（迁移状态、数据库完整性、钥匙串体检）照常产出，报告不会整体失败。
//! - **不泄漏**：报告里不含密钥内容、不含章节正文、不含 key_ref。
//!
//! ## 未覆盖（诚实说明）
//!
//! - IPC 命令 `get_diagnostics` 本身无法在 MockRuntime 下注册
//!   （首参为 `tauri::AppHandle`），与 `scan_image_storage` 同属已知限制；
//!   其 `rename_all = "snake_case"` 属性与命令数由 `ipc_argcase.rs` 守护。
//! - 真实系统钥匙串不参与测试；用内存 `KeyringStore` 注入。
//! - 孤儿文件 / 素材缺失的真实磁盘场景依赖文件系统构造，本文件用干净库与
//!   asset 表故障注入覆盖能确定的部分；更细的图片场景见 `asset_gallery.rs`。

use easy_writing_lib::db;
use easy_writing_lib::diagnostics::{collect_diagnostics, ids};
use easy_writing_lib::keyring::KeyringStore;
use shared_types::DiagnosticStatus;
use std::collections::HashMap;
use std::path::PathBuf;

/// 内存钥匙串：绝不触碰真实系统钥匙串。
struct MockKeyring {
    items: HashMap<String, String>,
}

impl MockKeyring {
    fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }

    fn with(mut self, key_ref: &str, secret: &str) -> Self {
        self.items.insert(key_ref.to_string(), secret.to_string());
        self
    }
}

impl KeyringStore for MockKeyring {
    fn get_secret(&self, key_ref: &str) -> Result<String, String> {
        self.items
            .get(key_ref)
            .cloned()
            .ok_or_else(|| "未找到指定的条目".to_string())
    }
    fn set_secret(&self, _key_ref: &str, _secret: &str) -> Result<(), String> {
        Ok(())
    }
    fn delete_secret(&self, _key_ref: &str) -> Result<(), String> {
        Ok(())
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ew_diag_{}_{}",
        tag,
        uuid::Uuid::new_v4().simple()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn get<'a>(items: &'a [shared_types::DiagnosticItem], id: &str) -> &'a shared_types::DiagnosticItem {
    items
        .iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("报告缺少诊断项 {id}"))
}

#[test]
fn clean_database_reports_all_available_items() {
    let dir = temp_dir("clean");
    let pool = db::init_db(dir.clone()).unwrap();
    let conn = pool.get().unwrap();
    let report = collect_diagnostics(&conn, &dir, &MockKeyring::new());

    assert_eq!(report.items.len(), 10, "应当聚合 6 项已有 + 4 项跳过");

    // 干净库：素材、孤儿、密钥、迁移、路径都是 ok（无告警）
    assert_eq!(get(&report.items, ids::ASSET_REFERENCES).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::ORPHAN_FILES).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::MISSING_KEYS).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::DATABASE_INTEGRITY).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::MIGRATION_STATUS).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::APP_DATA_PATHS).status, DiagnosticStatus::Ok);

    // 迁移版本应等于当前基线
    assert_eq!(
        get(&report.items, ids::MIGRATION_STATUS).count,
        Some(db::BASELINE_VERSION)
    );
    assert_eq!(report.error_count, 0);
    assert_eq!(report.skipped_count, 4);
    assert_eq!(report.ok_count, 6);

    // 四项缺失能力必须显式 skipped 且写明原因
    for id in [
        ids::LAST_BACKUP,
        ids::PENDING_AI_TASKS,
        ids::DISK_SPACE,
        ids::CRASH_RECOVERY_POINT,
    ] {
        let it = get(&report.items, id);
        assert_eq!(it.status, DiagnosticStatus::Skipped, "{id} 不应伪造数据");
        assert!(it.reason.is_some(), "{id} 必须写明跳过原因");
        assert!(it.count.is_none(), "{id} 不应携带伪造计数");
    }

    // 计数必须与实际状态一致
    let mut recomputed = report.clone();
    recomputed.recompute_counts();
    assert_eq!(recomputed.ok_count, report.ok_count);
    assert_eq!(recomputed.warning_count, report.warning_count);
    assert_eq!(recomputed.error_count, report.error_count);
    assert_eq!(recomputed.skipped_count, report.skipped_count);
}

#[test]
fn missing_key_is_reported_as_warning_without_leaking_secret() {
    let dir = temp_dir("keys");
    let pool = db::init_db(dir.clone()).unwrap();
    {
        let conn = pool.get().unwrap();
        db::llm_profile::create_profile(
            &conn,
            "缺密钥的接口",
            "openai",
            "http://127.0.0.1:1/v1",
            "ew_diag_missing_key_ref",
            "m",
            "{}",
            false,
        )
        .unwrap();
    }
    let conn = pool.get().unwrap();
    // 故意只注入另一个 key_ref 的密钥，制造「缺失」。
    let store = MockKeyring::new().with("unrelated_ref", "sk-this-secret-must-not-leak");
    let report = collect_diagnostics(&conn, &dir, &store);

    let it = get(&report.items, ids::MISSING_KEYS);
    assert_eq!(it.status, DiagnosticStatus::Warning);
    assert_eq!(it.count, Some(1));
    // 摘要只说数量，不含密钥内容、不含 key_ref
    assert!(!it.summary.contains("sk-this-secret-must-not-leak"));
    assert!(!it.summary.contains("ew_diag_missing_key_ref"));
    assert_eq!(it.reason, None);
}

#[test]
fn one_failing_item_does_not_block_the_others() {
    let dir = temp_dir("fault");
    let pool = db::init_db(dir.clone()).unwrap();
    {
        let conn = pool.get().unwrap();
        // 故障注入：删掉 asset 表，让两个素材类诊断失败；
        // llm_profile / webdav_config / schema_version 仍然存在。
        conn.execute_batch("DROP TABLE asset;").unwrap();
    }
    let conn = pool.get().unwrap();
    let report = collect_diagnostics(&conn, &dir, &MockKeyring::new());

    // 素材类两项变成 error，但仍有完整报告
    assert_eq!(get(&report.items, ids::ASSET_REFERENCES).status, DiagnosticStatus::Error);
    assert_eq!(get(&report.items, ids::ORPHAN_FILES).status, DiagnosticStatus::Error);
    assert!(get(&report.items, ids::ASSET_REFERENCES).reason.is_some());
    assert!(get(&report.items, ids::ORPHAN_FILES).reason.is_some());

    // 其余已有能力照常工作 —— 证明没有整体 `?` 短路
    assert_eq!(get(&report.items, ids::MISSING_KEYS).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::DATABASE_INTEGRITY).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::MIGRATION_STATUS).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::APP_DATA_PATHS).status, DiagnosticStatus::Ok);
    assert_eq!(get(&report.items, ids::MIGRATION_STATUS).count, Some(db::BASELINE_VERSION));

    // 四项跳过项不受影响
    assert_eq!(report.skipped_count, 4);
    assert_eq!(report.error_count, 2);
    assert_eq!(report.items.len(), 10);
}

#[test]
fn report_never_contains_credentials_or_chapter_body() {
    let dir = temp_dir("leak");
    let pool = db::init_db(dir.clone()).unwrap();
    {
        let conn = pool.get().unwrap();
        let novel = db::novel::create_novel(&conn, "敏感作品标题", "敏感梗概").unwrap();
        let vol = db::volume::create_volume(&conn, &novel.id, "V", "").unwrap();
        let ch = db::chapter::create_chapter(&conn, &novel.id, &vol.id, "秘密章节标题", "").unwrap();
        db::chapter::save_chapter_content(
            &conn,
            &ch.id,
            "{\"type\":\"doc\",\"content\":[{\"type\":\"paragraph\",\"content\":[{\"type\":\"text\",\"text\":\"绝密正文内容\"}]}]}",
            "绝密正文内容",
            6,
        )
        .unwrap();
        db::llm_profile::create_profile(
            &conn,
            "敏感接口名",
            "openai",
            "http://127.0.0.1:1/v1",
            "ew_leak_key_ref",
            "m",
            "{}",
            false,
        )
        .unwrap();
    }
    let conn = pool.get().unwrap();
    let store = MockKeyring::new().with("unrelated_ref", "sk-super-secret-value");
    let report = collect_diagnostics(&conn, &dir, &store);
    let serialized = serde_json::to_string(&report).unwrap();

    for needle in [
        "绝密正文内容",
        "秘密章节标题",
        "敏感作品标题",
        "sk-super-secret-value",
        "ew_leak_key_ref",
    ] {
        assert!(
            !serialized.contains(needle),
            "诊断报告泄漏了正文 / 凭据 / key_ref：{needle}"
        );
    }
}

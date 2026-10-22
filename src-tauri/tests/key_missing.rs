//! 密钥缺失统一处理的回归测试（任务书 T05 / PR-06）。
//!
//! ## 覆盖什么
//!
//! - **统一错误码**：AI 调用与 WebDAV 取密码在密钥缺失时都返回
//!   以 `KEY_MISSING:` 开头的错误，而不是各自的自由中文文本。
//! - **覆盖面**：体检同时检查 `llm_profile.key_ref` 与 `webdav_config.pwd_ref`
//!   （旧实现只扫前者）。
//! - **可 mock**：钥匙串访问被抽象成 `KeyringStore` trait，
//!   测试用内存实现，**不触碰真实系统钥匙串**。
//! - **不误报**：`key_ref` 为空的记录（从来没配过密钥）不算「缺失」。
//! - **恢复报告**：恢复完成后列出的缺失项带 `scope`，前端能分别引导。
//!
//! ## 数据安全
//!
//! 数据库在 `std::env::temp_dir()` 下的隔离目录中；钥匙串是内存里的假实现，
//! 既不读也不写真实系统钥匙串。

use easy_writing_lib::db;
use easy_writing_lib::keyring::KeyringStore;
use easy_writing_lib::keys;
use std::collections::HashMap;

/// 内存钥匙串：不触碰真实系统钥匙串。
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
    fn set_secret(&self, key_ref: &str, secret: &str) -> Result<(), String> {
        let mut items = self.items.clone();
        items.insert(key_ref.to_string(), secret.to_string());
        Ok(())
    }
    fn delete_secret(&self, _key_ref: &str) -> Result<(), String> {
        Ok(())
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ew_keys_{}_{}",
        tag,
        uuid::Uuid::new_v4().simple()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn seeded(dir: &std::path::Path) -> db::DbPool {
    let pool = db::init_db(dir.to_path_buf()).unwrap();
    let conn = pool.get().unwrap();
    // 两个模型接口：一个钥匙串里有，一个没有
    db::llm_profile::create_profile(&conn, "有密钥的接口", "openai", "http://x/v1", "key-ok", "m", "{}", false).unwrap();
    db::llm_profile::create_profile(&conn, "缺密钥的接口", "openai", "http://x/v1", "key-missing", "m", "{}", false).unwrap();
    // 一个从来没用过的接口（key_ref 为空）：不是「缺失」
    db::llm_profile::create_profile(&conn, "未配置密钥的接口", "openai", "http://x/v1", "", "m", "{}", false).unwrap();
    // WebDAV 配置：密码凭据缺失
    conn.execute(
        "INSERT INTO webdav_config (id, url, username, pwd_ref, remote_dir, updated_at)
         VALUES ('default_webdav', 'https://dav.example/', 'u', 'webdav_default_pwd', '/bk', datetime('now'))
         ON CONFLICT(id) DO NOTHING",
        [],
    )
    .unwrap();
    drop(conn);
    pool
}

#[test]
fn missing_llm_key_returns_key_missing_code() {
    let store = MockKeyring::new().with("key-ok", "sk-123");
    let err = keys::require_llm_key(&store, "key-missing").unwrap_err();
    assert!(
        keys::is_key_missing(&err),
        "密钥缺失必须返回可判定的错误码，实际：{err}"
    );
    assert!(err.starts_with("KEY_MISSING:"));
    assert!(err.contains("llm_profile"));
}

#[test]
fn missing_webdav_password_returns_key_missing_code() {
    let store = MockKeyring::new();
    let err = keys::require_webdav_password(&store, "webdav_default_pwd").unwrap_err();
    assert!(keys::is_key_missing(&err));
    assert!(err.contains("webdav"));
}

#[test]
fn present_keys_are_returned_unchanged() {
    let store = MockKeyring::new()
        .with("key-ok", "sk-123")
        .with("webdav_default_pwd", "pw");
    assert_eq!(
        keys::require_llm_key(&store, "key-ok").unwrap(),
        "sk-123".to_string()
    );
    assert_eq!(
        keys::require_webdav_password(&store, "webdav_default_pwd").unwrap(),
        "pw".to_string()
    );
}

#[test]
fn key_missing_is_machine_detectable() {
    assert!(keys::is_key_missing("KEY_MISSING: llm_profile · 缺了"));
    assert!(!keys::is_key_missing("网络连接失败"));
    assert!(!keys::is_key_missing("401 Unauthorized"));
    assert!(!keys::is_key_missing(""));
}

#[test]
fn scan_covers_llm_profiles_and_webdav_config() {
    let dir = temp_dir("scan");
    let pool = seeded(&dir);
    let conn = pool.get().unwrap();
    let store = MockKeyring::new().with("key-ok", "sk-123");

    let missing = keys::scan_missing_keys(&conn, &store);
    let llm: Vec<&keys::MissingKey> = missing
        .iter()
        .filter(|m| m.scope == keys::SCOPE_LLM_PROFILE)
        .collect();
    let webdav: Vec<&keys::MissingKey> = missing
        .iter()
        .filter(|m| m.scope == keys::SCOPE_WEBDAV)
        .collect();

    assert_eq!(llm.len(), 1, "只应报出缺密钥的那个模型接口");
    assert_eq!(llm[0].name, "缺密钥的接口");
    assert_eq!(webdav.len(), 1, "WebDAV 密码缺失也必须被报出");
    assert_eq!(webdav[0].key_ref, "webdav_default_pwd");
}

#[test]
fn scan_ignores_empty_key_ref() {
    let dir = temp_dir("scan_empty");
    let pool = seeded(&dir);
    let conn = pool.get().unwrap();
    let store = MockKeyring::new().with("key-ok", "sk-123");

    let missing = keys::scan_missing_keys(&conn, &store);
    assert!(
        !missing.iter().any(|m| m.name == "未配置密钥的接口"),
        "从来没配过密钥的接口不算「缺失」：{missing:?}"
    );
}

#[test]
fn scan_uses_injected_store_only() {
    // 体检必须走注入的 store：真实的系统钥匙串里没有这些 key_ref，
    // 如果实现偷偷用了 system()，本用例会因为「意外报了缺失」而失败。
    let dir = temp_dir("scan_injected");
    let pool = seeded(&dir);
    let conn = pool.get().unwrap();

    let complete = MockKeyring::new()
        .with("key-ok", "sk-123")
        .with("key-missing", "sk-456")
        .with("webdav_default_pwd", "pw");
    assert!(
        keys::scan_missing_keys(&conn, &complete).is_empty(),
        "钥匙齐全时不应报出任何缺失"
    );
}

#[test]
fn restore_report_lists_webdav_dangling_key() {
    // 恢复完成后列出的缺失项必须覆盖 WebDAV 密码（旧实现只报模型接口）。
    let dir = temp_dir("restore_dangling");
    let pool = seeded(&dir);
    let conn = pool.get().unwrap();
    let store = MockKeyring::new().with("key-ok", "sk-123");

    let missing = keys::scan_missing_keys(&conn, &store);
    let dangling: Vec<shared_types::DanglingKeyInfo> = missing
        .into_iter()
        .map(|m| shared_types::DanglingKeyInfo {
            profile_id: m.id,
            profile_name: m.name,
            key_ref: m.key_ref,
            scope: m.scope,
        })
        .collect();

    assert!(dangling.iter().any(|d| d.scope == "webdav"));
    assert!(dangling.iter().any(|d| d.scope == "llm_profile"));
}

#[test]
fn dangling_key_info_scope_defaults_for_legacy_values() {
    // 旧结构没有 scope 字段：反序列化时必须按模型接口处理，不能崩。
    let legacy = r#"{"profile_id":"p1","profile_name":"旧接口","key_ref":"k1"}"#;
    let parsed: shared_types::DanglingKeyInfo = serde_json::from_str(legacy).unwrap();
    assert_eq!(parsed.scope, "llm_profile");
}

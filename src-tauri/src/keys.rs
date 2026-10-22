//! 密钥缺失的统一处理。
//!
//! ## 为什么要有这一层
//!
//! 换设备之后，数据库里存的 `key_ref` 会指向本机钥匙串里**并不存在**的条目
//! （备份不含任何密钥，这是设计如此）。此时应用如果继续跑下去，
//! 作者看到的是「网络错误」「401」「连接超时」这类**指向错误方向**的提示，
//! 于是去查网络、查代理、查服务端 —— 而真正的原因是「这台机器上没有那条密钥」。
//!
//! 旧实现里这件事散在三处，各写各的中文自由文本：
//!
//! - `agent/dispatcher.rs`：「当前绑定的模型接口未检测到有效的 API Key 密钥……」
//! - `commands/config_cmd.rs`：「当前接口未检测到有效的 API Key 密钥……」
//! - `commands/sync_cmd.rs`：「未在系统钥匙串中找到 WebDAV 密码凭据」
//!
//! 三处文案不同、机器无法判定、覆盖的密钥种类也不同
//! （只扫 `llm_profile.key_ref`，不扫 `webdav_config.pwd_ref`）。
//!
//! 现在统一成：**错误串以 `KEY_MISSING:` 开头** —— 前缀是给程序看的判据，
//! 后半句是给作者看的人话。覆盖面扩到「启动时 + 恢复后」，
//! 且模型接口与 WebDAV 两类密钥都在其中。

use crate::keyring::KeyringStore;
use rusqlite::Connection;

/// 密钥缺失的机器可判定错误码。
///
/// 约定：错误串以 `KEY_MISSING: ` 开头。
/// 前端据此把「缺少密钥」与「网络错误 / 服务端错误」区分开，
/// 并把作者引到对应的设置项，而不是让他们去查网络。
pub const KEY_MISSING_CODE: &str = "KEY_MISSING";

/// 密钥作用域：模型接口的 API Key。
pub const SCOPE_LLM_PROFILE: &str = "llm_profile";
/// 密钥作用域：WebDAV 密码。
pub const SCOPE_WEBDAV: &str = "webdav";

/// 构造一条「密钥缺失」错误。
pub fn key_missing(scope: &str, detail: &str) -> String {
    format!("{KEY_MISSING_CODE}: {scope} · {detail}")
}

/// 判断一个错误串是不是「密钥缺失」。
pub fn is_key_missing(message: &str) -> bool {
    message.starts_with(KEY_MISSING_CODE)
}

/// 一条缺失的密钥。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingKey {
    /// 密钥作用域：`llm_profile` 或 `webdav`。
    pub scope: String,
    /// 对应配置项的 id。
    pub id: String,
    /// 面向作者的名称（模型接口名 / 「WebDAV 密码」）。
    pub name: String,
    /// 钥匙串中的引用键名。**不包含密钥内容**。
    pub key_ref: String,
}

/// 扫描所有需要钥匙串的密钥，找出缺失的。
///
/// 覆盖面是「启动时 + 恢复后」共用的同一份逻辑：
/// 模型接口的 `llm_profile.key_ref` 与 WebDAV 的 `webdav_config.pwd_ref`。
/// 旧实现只扫前者，且只在恢复流程里调用。
pub fn scan_missing_keys(conn: &Connection, store: &dyn KeyringStore) -> Vec<MissingKey> {
    let mut missing = Vec::new();

    // 模型接口
    let profiles: Vec<(String, String, String)> =
        query_rows(conn, "SELECT id, name, key_ref FROM llm_profile", |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });
    for (id, name, key_ref) in profiles {
        // key_ref 为空 = 从来没配过密钥，不是「缺失」，不报。
        if !key_ref.trim().is_empty() && store.get_secret(&key_ref).is_err() {
            missing.push(MissingKey {
                scope: SCOPE_LLM_PROFILE.to_string(),
                id,
                name,
                key_ref,
            });
        }
    }

    // WebDAV 密码
    let webdav: Vec<(String, String)> =
        query_rows(conn, "SELECT id, pwd_ref FROM webdav_config", |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });
    for (id, pwd_ref) in webdav {
        if !pwd_ref.trim().is_empty() && store.get_secret(&pwd_ref).is_err() {
            missing.push(MissingKey {
                scope: SCOPE_WEBDAV.to_string(),
                id,
                name: "WebDAV 密码".to_string(),
                key_ref: pwd_ref,
            });
        }
    }

    missing
}

/// 读一张表的三列，返回空列表表示读不到。
///
/// 体检失败**不能**让整个应用起不来：读不到配置只是「暂时查不出缺哪些密钥」，
/// 不是「一定没有缺失」。真出问题时设置页的徽标少亮一个，而应用照常可用。
fn query_rows<T>(
    conn: &Connection,
    sql: &str,
    map: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Vec<T> {
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], map) else {
        return Vec::new();
    };
    rows.filter_map(|r| r.ok()).collect()
}

/// 取模型接口的 API Key；缺失时返回 `KEY_MISSING` 错误。
///
/// 旧实现用 `unwrap_or_default()` 把「钥匙串报错」与「密钥为空」压成同一个
/// 空串，再靠一句中文提示拦下。这里把两种情况分别翻译成不同的文案，
/// 但都带同一个错误码前缀。
pub fn require_llm_key(store: &dyn KeyringStore, key_ref: &str) -> Result<String, String> {
    if key_ref.trim().is_empty() {
        return Err(key_missing(
            SCOPE_LLM_PROFILE,
            "该模型接口配置没有绑定密钥，请重新填写 API Key。",
        ));
    }
    match store.get_secret(key_ref) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        Ok(_) => Err(key_missing(
            SCOPE_LLM_PROFILE,
            "钥匙串中的 API Key 为空，请重新填写。",
        )),
        Err(e) => Err(key_missing(
            SCOPE_LLM_PROFILE,
            &format!("系统钥匙串中找不到该 API Key（{e}），请重新填写。"),
        )),
    }
}

/// 取 WebDAV 密码；缺失时返回 `KEY_MISSING` 错误。
pub fn require_webdav_password(
    store: &dyn KeyringStore,
    pwd_ref: &str,
) -> Result<String, String> {
    if pwd_ref.trim().is_empty() {
        return Err(key_missing(
            SCOPE_WEBDAV,
            "尚未保存 WebDAV 密码，请在系统设置中填写。",
        ));
    }
    match store.get_secret(pwd_ref) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        Ok(_) => Err(key_missing(
            SCOPE_WEBDAV,
            "钥匙串中的 WebDAV 密码为空，请重新填写。",
        )),
        Err(e) => Err(key_missing(
            SCOPE_WEBDAV,
            &format!("系统钥匙串中找不到 WebDAV 密码（{e}），请重新填写。"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockKeyring {
        items: HashMap<String, String>,
    }

    impl MockKeyring {
        fn new() -> Self {
            Self {
                items: HashMap::new(),
            }
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
        fn delete_secret(&self, key_ref: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn key_missing_is_machine_detectable() {
        let msg = key_missing(SCOPE_LLM_PROFILE, "缺了");
        assert!(is_key_missing(&msg));
        assert!(msg.starts_with("KEY_MISSING:"));
        assert!(!is_key_missing("网络连接失败"));
        assert!(!is_key_missing(""));
    }

    #[test]
    fn require_llm_key_distinguishes_empty_ref_and_missing_entry() {
        let store = MockKeyring::new();
        let err = require_llm_key(&store, "").unwrap_err();
        assert!(is_key_missing(&err));

        let err = require_llm_key(&store, "llm-key-1").unwrap_err();
        assert!(is_key_missing(&err));
        assert!(err.contains(SCOPE_LLM_PROFILE));
    }

    #[test]
    fn require_webdav_password_reports_webdav_scope() {
        let store = MockKeyring::new();
        let err = require_webdav_password(&store, "webdav_default_pwd").unwrap_err();
        assert!(is_key_missing(&err));
        assert!(err.contains(SCOPE_WEBDAV));
    }
}

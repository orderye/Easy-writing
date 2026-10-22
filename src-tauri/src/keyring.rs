//! 系统钥匙串访问。
//!
//! ## 为什么要有 trait
//!
//! 旧实现只有三个自由函数，直接调用 `keyring` crate。这带来两个问题：
//!
//! 1. **无法测试**：「钥匙串里少了这条密钥」是本任务最核心的验收场景，
//!    但自由函数背后是真实的系统钥匙串，测试既不能删条目也不能伪造缺失；
//! 2. **无法区分错误**：`get_secret` 的失败既可能是「条目不存在」，
//!    也可能是「钥匙串被锁」「用户拒绝授权」，两者被压成同一个 `Err`。
//!
//! 现在把「怎么存」抽象成 [`KeyringStore`]，被测逻辑一律接受
//! `&dyn KeyringStore`，生产环境传 [`SystemKeyring`]，测试传内存实现。
//! 「条目不存在」与「其它错误」在预检层被分别翻译成不同的提示文案，
//! 但都归到同一个机器可判定的错误码 `KEY_MISSING`（见 [`crate::keys`]）。

use keyring::Entry;

const SERVICE_NAME: &str = "easy-writing";

/// 钥匙串访问的抽象。
///
/// 之所以要它：密钥缺失是**换设备后最常见**的问题，而它的验收标准
/// 「删掉钥匙串条目后，调用必须返回 KEY_MISSING 而不是网络错误」
/// 只有在能控制钥匙串内容时才测得出来。
pub trait KeyringStore: Send + Sync {
    /// 读取密钥。条目不存在时返回 `Err`。
    fn get_secret(&self, key_ref: &str) -> Result<String, String>;
    /// 写入密钥。
    fn set_secret(&self, key_ref: &str, secret: &str) -> Result<(), String>;
    /// 删除密钥。条目不存在时应当视为成功。
    fn delete_secret(&self, key_ref: &str) -> Result<(), String>;
}

/// 真实的系统钥匙串（macOS Keychain / Windows Credential Manager）。
pub struct SystemKeyring;

impl KeyringStore for SystemKeyring {
    fn get_secret(&self, key_ref: &str) -> Result<String, String> {
        let entry = Entry::new(SERVICE_NAME, key_ref).map_err(|e| e.to_string())?;
        entry.get_password().map_err(|e| e.to_string())
    }

    fn set_secret(&self, key_ref: &str, secret: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, key_ref).map_err(|e| e.to_string())?;
        entry.set_password(secret).map_err(|e| e.to_string())
    }

    fn delete_secret(&self, key_ref: &str) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, key_ref).map_err(|e| e.to_string())?;
        entry.delete_credential().map_err(|e| e.to_string())
    }
}

/// 系统钥匙串实例。
pub fn system() -> &'static SystemKeyring {
    &SystemKeyring
}

// ---------------------------------------------------------------------------
// 便捷函数：保持旧调用点不变
// ---------------------------------------------------------------------------

pub fn set_secret(key_ref: &str, secret: &str) -> Result<(), String> {
    system().set_secret(key_ref, secret)
}

pub fn get_secret(key_ref: &str) -> Result<String, String> {
    system().get_secret(key_ref)
}

pub fn delete_secret(key_ref: &str) -> Result<(), String> {
    system().delete_secret(key_ref)
}

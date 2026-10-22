//! 密钥缺失错误的统一解读。
//!
//! 后端把「本机钥匙串里没有那条密钥」统一成以 `KEY_MISSING:` 开头的错误串
//! （见 `crate::keys`）。前端据此把「缺少密钥」与「网络错误 / 服务端错误」
//! 区分开：前者要引导作者去设置项补密钥，后者才该去查网络。
//!
//! 错误串格式：`KEY_MISSING: <scope> · <人话说明>`

/// 密钥缺失的机器可判定前缀（与后端 `crate::keys::KEY_MISSING_CODE` 对应）。
pub const KEY_MISSING_CODE: &str = "KEY_MISSING";

/// 判断一个错误串是不是「密钥缺失」。
pub fn is_key_missing(message: &str) -> bool {
    message.starts_with(KEY_MISSING_CODE)
}

/// 把密钥缺失错误翻译成给作者看的话。
///
/// 返回 `None` 表示这不是密钥缺失错误，调用方应按原样处理。
pub fn explain_key_missing(message: &str) -> Option<String> {
    if !is_key_missing(message) {
        return None;
    }
    let rest = message
        .trim_start_matches(KEY_MISSING_CODE)
        .trim_start_matches(':')
        .trim();
    let (scope, detail) = match rest.split_once('·') {
        Some((s, d)) => (s.trim(), d.trim()),
        None => ("", rest),
    };
    let target = if scope == "webdav" {
        "「WebDAV 整库同步」"
    } else {
        "「大模型 API 接口」"
    };
    Some(format!(
        "缺少密钥：{detail}\n请到{target}重新填写密钥后重试（换设备后本机钥匙串里通常没有这些条目）。"
    ))
}

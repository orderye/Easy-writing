//! 应用偏好（app_setting 表）的读写入口。
//!
//! 原先 config_modal.rs 与 pref_tab.rs 各持有一份完全相同的 `read_setting` /
//! `write_setting`（标签页拆分时为了各自可编译而复制）。两处副本意味着
//! 改一次设置读写逻辑要动两个文件、且容易漏改其一处——这里收敛为唯一定义。
//!
//! 只依赖 `invoke_cmd` 构建器，命令名与参数键名（`get_app_setting` /
//! `set_app_setting` 的 `key`、`value`）与后端命令签名一致。

use crate::tauri_api::invoke_cmd;

/// 读取一条应用偏好；键不存在时返回 `None`。
pub async fn read_setting(key: &str) -> Result<Option<String>, String> {
    invoke_cmd("get_app_setting")
        .arg("key", key.to_string())
        .send::<Option<String>>()
        .await
}

/// 写入一条应用偏好（存在则覆盖）。
pub async fn write_setting(key: &str, value: String) -> Result<(), String> {
    invoke_cmd("set_app_setting")
        .arg("key", key.to_string())
        .arg("value", value)
        .send_unit()
        .await
}

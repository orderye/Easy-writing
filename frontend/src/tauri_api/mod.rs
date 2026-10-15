//! Tauri IPC 调用封装。
//!
//! 【为什么用构建器而不是参数结构体】
//! 历史上每个调用点都手写一个只用于序列化的参数结构体：
//!
//! ```ignore
//! #[derive(Serialize)]
//! struct DeleteChapterArgs { id: String }
//! // ...
//! invoke::<(), _>("delete_chapter", &DeleteChapterArgs { id }).await
//! ```
//!
//! 这类结构体只是把后端 `#[tauri::command(rename_all = "snake_case")]` 已经约定了的
//! 键名再抄一遍，既冗长又容易与后端漂移（键名写错要到运行时才暴露）。
//! `invoke_cmd` 构建器把键名与值写在一处：
//!
//! ```ignore
//! let _: () = invoke_cmd("delete_chapter").arg("id", id).await?;
//! let n: Novel = invoke_cmd("create_novel")
//!     .arg("title", title)
//!     .arg("synopsis", synopsis)
//!     .await?;
//! ```
//!
//! 可选参数用 `.opt(..)`（`None` → JSON `null`，对应后端 `Option<T>`）；
//! 返回 `()` 的命令用 `.send_unit()`。键名契约仍由 `ipc_argcase.rs` 与
//! `command_key_sweep.rs` 两个测试守护。

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function tauri_invoke_raw(cmd, args) {
  const payload = (args !== null && args !== undefined && typeof args === 'object') ? args : {};
  if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {
    return window.__TAURI__.core.invoke(cmd, payload);
  }
  if (window.__TAURI__ && typeof window.__TAURI__.invoke === 'function') {
    return window.__TAURI__.invoke(cmd, payload);
  }
  if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {
    return window.__TAURI_INTERNALS__.invoke(cmd, payload);
  }
  return Promise.reject("Tauri IPC bridge not found in current WebView window.");
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn tauri_invoke_raw(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

pub async fn invoke<T: DeserializeOwned, A: Serialize>(cmd: &str, args: &A) -> Result<T, String> {
    let args_val = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    match tauri_invoke_raw(cmd, args_val).await {
        Ok(val) => serde_wasm_bindgen::from_value(val).map_err(|e| e.to_string()),
        Err(err) => Err(format!("{:?}", err)),
    }
}

pub async fn invoke_no_args<T: DeserializeOwned>(cmd: &str) -> Result<T, String> {
    match tauri_invoke_raw(cmd, JsValue::NULL).await {
        Ok(val) => serde_wasm_bindgen::from_value(val).map_err(|e| e.to_string()),
        Err(err) => Err(format!("{:?}", err)),
    }
}
/// 链式构造调用参数的构建器。
///
/// 之所以不用宏展开字段表：`macro_rules!` 对 `{ a, b: expr }` 这种
/// 简写与显式键名混排的片段存在匹配歧义（`$v:expr` 后不能直接跟 `,`），
/// 构建器形态没有这个问题，且类型推断与普通 Rust 代码一致。
///
/// 用法：
/// ```ignore
/// let _: () = invoke_cmd("delete_chapter").arg("id", id).await?;
/// let n: Novel = invoke_cmd("create_novel")
///     .arg("title", title)
///     .arg("synopsis", synopsis)
///     .await?;
/// let _: () = invoke_cmd("save_lexicon_term")
///     .arg("novel_id", novel_id)
///     .arg("term", term)
///     .arg("source_id", None::<String>)   // 可选值显式标注或省略后调 .opt()
///     .await?;
/// ```
pub struct InvokeBuilder {
    cmd: &'static str,
    map: serde_json::Map<String, serde_json::Value>,
}

/// 开始构造一次 IPC 调用。`cmd` 为 `&'static str`，避免运行期拼命令名。
pub fn invoke_cmd(cmd: &'static str) -> InvokeBuilder {
    InvokeBuilder {
        cmd,
        map: serde_json::Map::new(),
    }
}

impl InvokeBuilder {
    /// 添加一个参数；键名由调用方给出，与后端 snake_case 形参同名。
    pub fn arg(mut self, key: &str, value: impl Serialize) -> Self {
        self.map.insert(
            key.to_string(),
            serde_json::to_value(&value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    /// 添加一个可选参数：`None` 序列化为 JSON `null`（与后端 `Option<T>` 对应）。
    pub fn opt<T: Serialize>(mut self, key: &str, value: Option<T>) -> Self {
        self.map.insert(
            key.to_string(),
            match value {
                Some(v) => serde_json::to_value(&v).unwrap_or(serde_json::Value::Null),
                None => serde_json::Value::Null,
            },
        );
        self
    }

    /// 发起调用，返回类型由调用处标注。
    pub async fn send<T: DeserializeOwned>(self) -> Result<T, String> {
        invoke(self.cmd, &serde_json::Value::Object(self.map)).await
    }

    /// 发起调用并忽略返回值（后端返回 `()` 时使用）。
    pub async fn send_unit(self) -> Result<(), String> {
        self.send::<()>().await
    }
}

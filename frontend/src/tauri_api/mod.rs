use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function tauri_invoke_raw(cmd, args) {
  if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  if (window.__TAURI__ && typeof window.__TAURI__.invoke === 'function') {
    return window.__TAURI__.invoke(cmd, args);
  }
  if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {
    return window.__TAURI_INTERNALS__.invoke(cmd, args);
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

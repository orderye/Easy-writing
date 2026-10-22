//! Tauri 事件订阅（前端侧）。
//!
//! 导出进度是**从后端推给前端**的，因此前端要能监听 Tauri 事件。
//! 仓库里此前没有事件监听的先例，所以把 JS 侧的差异全部封在这个文件里，
//! 其它地方只看到 Rust 函数。
//!
//! 用 `window.__TAURI__` 而不是 `@tauri-apps/api` 的 npm 包：
//! 应用已开启 `withGlobalTauri`（`src-tauri/tauri.conf.json`），
//! 再引入一份 npm 包只会多一次构建依赖。

use std::cell::RefCell;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function tauri_event_listen(event_name, callback) {
  const tauri = window.__TAURI__;
  if (!tauri || !tauri.event || typeof tauri.event.listen !== 'function') {
    console.warn('[Events] 当前窗口没有可用的 Tauri 事件接口');
    return;
  }
  // listen 返回的 unlisten 是 Promise；本应用的事件订阅与窗口同寿命，
  // 不需要退订，因此这里刻意不接它。
  tauri.event.listen(event_name, (e) => {
    try {
      callback(e && e.payload !== undefined ? e.payload : e);
    } catch (err) {
      console.error('[Events] 事件回调失败', err);
    }
  });
}
"#)]
extern "C" {
    fn tauri_event_listen_raw(event_name: &str, callback: &js_sys::Function);
}

thread_local! {
    /// 订阅回调的保活区。
    ///
    /// `Closure` 被丢弃时，wasm-bindgen 会同时释放 JS 侧的函数对象；
    /// 如果没人持有它，回调会在下一次 GC 时消失，事件就再也收不到了。
    /// 这里把闭包存起来，让它活到页面结束。
    static LISTENERS: RefCell<Vec<Closure<dyn FnMut(JsValue)>>> = RefCell::new(Vec::new());
}

/// 订阅一个 Tauri 事件，回调直接用 Rust 闭包接收载荷。
///
/// 这是给 Rust 侧用的入口：调用方不必知道 `js_sys::Function` 怎么构造。
pub fn listen_with<F>(event_name: &str, mut callback: F)
where
    F: FnMut(JsValue) + 'static,
{
    let closure = Closure::<dyn FnMut(JsValue)>::new(move |value: JsValue| {
        callback(value);
    });
    let function = js_sys::Function::from(closure.as_ref().clone());
    LISTENERS.with(|listeners| listeners.borrow_mut().push(closure));
    tauri_event_listen_raw(event_name, &function);
}

//! 图片上传流程（编辑器工具栏与图库共用的一份实现）。
//!
//! 【为什么单独成文件】
//! 这段流程（建 input → FileReader 读 base64 → save_image_asset 落库 →
//! 取 APPDATA → 转 asset:// URL）原先只写在工具栏里。图库的空态也要能上传，
//! 若再抄一份，就是同一段逻辑的第二份实现——本仓已因「复制两份」付过一次代价
//! （read_setting/write_setting 曾在两处各存一份）。
//!
//! 抽取时按**纯搬迁**处理：accept 列表、错误文案、「失败不影响正文编辑」这些
//! 行为与抽取前逐字一致。

use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// 上传过程中的状态变化。由调用方决定如何呈现（工具栏显示在按钮旁，
/// 图库显示在标题栏），因此这里只报事件，不碰任何信号。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadEvent {
    /// 已选中文件，开始读取与落库。
    Started,
    /// 失败，附带完整的用户可见文案（含「✕」前缀，与抽取前一致）。
    Failed(String),
}

/// 把本地选中的图片文件读成 base64（去掉 data URL 前缀，只留纯 base64）。
///
/// 用 FileReader 在浏览器侧完成编码，避免把二进制经 IPC 传输。
fn read_file_as_base64(file: &web_sys::File) -> js_sys::Promise {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    js_sys::Promise::new(&mut |resolve, reject| {
        let reader = match web_sys::FileReader::new() {
            Ok(r) => r,
            Err(_) => {
                let _ = reject.call1(&wasm_bindgen::JsValue::NULL, &"无法创建 FileReader".into());
                return;
            }
        };
        let reader_for_load = reader.clone();
        let onload = Closure::once_into_js(move || {
            let result = reader_for_load
                .result()
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            // 结果形如 "data:image/png;base64,XXXX"，只取逗号后的部分
            let b64 = result.split(',').nth(1).unwrap_or("").to_string();
            let _ = resolve.call1(&wasm_bindgen::JsValue::NULL, &b64.into());
        });
        reader.set_onload(Some(onload.unchecked_ref()));

        let reject_for_error = reject.clone();
        let onerror = Closure::once_into_js(move || {
            let _ = reject_for_error.call1(&wasm_bindgen::JsValue::NULL, &"读取图片文件失败".into());
        });
        reader.set_onerror(Some(onerror.unchecked_ref()));

        if reader.read_as_data_url(file).is_err() {
            let _ = reject.call1(&wasm_bindgen::JsValue::NULL, &"读取图片文件失败".into());
        }
    })
}

/// 弹出文件选择器 → 落库为图片资产 → 以「(rel_path, name)」回调调用方。
///
/// - on_event：进度 / 失败事件（呈现方式由调用方决定）。
/// - on_saved：落库成功，参数为相对路径（形如 images/<sha256>.png）与原始文件名。
///
/// 调用方需自行做「正在上传中则忽略」的重入保护（工具栏与图库各自持有该状态）。
pub fn pick_and_save_image<F, G>(app_ctx: AppContext, on_event: F, on_saved: G)
where
    F: Fn(UploadEvent) + 'static,
    G: Fn(String, String) + 'static,
{
    let Some(novel) = app_ctx.current_novel.get() else {
        on_event(UploadEvent::Failed("✕ 请先选择或新建一个作品".to_string()));
        return;
    };
    let novel_id = novel.id;

    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Ok(input) = document.create_element("input") else {
        return;
    };
    let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() else {
        return;
    };
    input.set_type("file");
    input.set_accept("image/png,image/jpeg,image/webp,image/gif");

    let input_for_change = input.clone();
    let onchange = wasm_bindgen::closure::Closure::once_into_js(move || {
        let Some(files) = input_for_change.files() else {
            return;
        };
        let Some(file) = files.get(0) else { return };

        let name = file.name();
        let mime = if file.type_().is_empty() {
            "image/png".to_string()
        } else {
            file.type_()
        };

        on_event(UploadEvent::Started);

        leptos::task::spawn_local(async move {
            let b64_val = wasm_bindgen_futures::JsFuture::from(read_file_as_base64(&file)).await;
            let Ok(b64_val) = b64_val else {
                on_event(UploadEvent::Failed("✕ 读取图片失败".to_string()));
                return;
            };
            let base64_data = b64_val.as_string().unwrap_or_default();
            if base64_data.is_empty() {
                on_event(UploadEvent::Failed("✕ 图片内容为空".to_string()));
                return;
            }

            // 落库为资产，拿相对路径
            let res = invoke_cmd("save_image_asset")
                .arg("novel_id", novel_id.clone())
                .arg("name", name.clone())
                .arg("mime", mime.clone())
                .arg("base64_data", base64_data)
                .send::<String>()
                .await;

            match res {
                Ok(rel_path) => on_saved(rel_path, name),
                Err(e) => on_event(UploadEvent::Failed(format!("✕ 保存图片失败：{}", e))),
            }
        });
    });
    input.set_onchange(Some(onchange.unchecked_ref()));
    input.click();
}

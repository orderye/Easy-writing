use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

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

#[component]
pub fn EditorToolbar(app_ctx: AppContext) -> impl IntoView {
    let is_uploading = RwSignal::new(false);
    let upload_msg = RwSignal::new(String::new());

    // 插图：选择文件 → 读 base64 → 落库为 asset → 插入 asset:// URL
    let on_insert_image = move |_| {
        if is_uploading.get() {
            return;
        }
        let Some(novel) = app_ctx.current_novel.get() else {
            upload_msg.set("✕ 请先选择或新建一个作品".to_string());
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

            is_uploading.set(true);
            upload_msg.set("正在保存图片...".to_string());

            leptos::task::spawn_local(async move {
                let b64_val = wasm_bindgen_futures::JsFuture::from(read_file_as_base64(&file)).await;
                let Ok(b64_val) = b64_val else {
                    upload_msg.set("✕ 读取图片失败".to_string());
                    is_uploading.set(false);
                    return;
                };
                let base64_data = b64_val.as_string().unwrap_or_default();
                if base64_data.is_empty() {
                    upload_msg.set("✕ 图片内容为空".to_string());
                    is_uploading.set(false);
                    return;
                }

                // 1. 落库为资产，拿相对路径
                let res = invoke_cmd("save_image_asset")
                    .arg("novel_id", novel_id.clone())
                    .arg("name", name.clone())
                    .arg("mime", mime.clone())
                    .arg("base64_data", base64_data)
                    .send::<String>()
                    .await;

                match res {
                    Ok(rel_path) => {
                        // 2. 相对路径 → asset:// URL，插入编辑器
                        let app_dir = invoke_no_args::<String>("get_app_data_dir")
                            .await
                            .unwrap_or_default();
                        let url = crate::bridge::asset::resolve_asset_url(&rel_path, &app_dir);
                        tiptap::insert_image(&url, &name);
                        app_ctx.save_status.set("dirty".to_string());
                        upload_msg.set("✓ 图片已插入".to_string());
                    }
                    Err(e) => upload_msg.set(format!("✕ 保存图片失败：{}", e)),
                }
                is_uploading.set(false);
            });
        });
        input.set_onchange(Some(onchange.unchecked_ref()));
        input.click();
    };

    view! {
        <div class="h-9 border-b border-slate-200 dark:border-slate-700 bg-slate-50/80 dark:bg-slate-800/80 px-4 flex items-center gap-1 select-none z-10 shrink-0 overflow-x-auto">
            // Headings
            <button
                class="px-2 py-0.5 text-xs font-semibold text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="一级标题"
                on:click=move |_| tiptap::toggle_heading(1)
            >
                "H1"
            </button>
            <button
                class="px-2 py-0.5 text-xs font-semibold text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="二级标题"
                on:click=move |_| tiptap::toggle_heading(2)
            >
                "H2"
            </button>
            <button
                class="px-2 py-0.5 text-xs font-semibold text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="三级标题"
                on:click=move |_| tiptap::toggle_heading(3)
            >
                "H3"
            </button>

            <span class="text-slate-300 dark:text-slate-600 mx-1">"|"</span>

            // Inline styles
            <button
                class="px-2 py-0.5 text-xs font-bold text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="加粗 (⌘B)"
                on:click=move |_| tiptap::toggle_bold()
            >
                "B"
            </button>
            <button
                class="px-2 py-0.5 text-xs italic font-serif text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="斜体 (⌘I)"
                on:click=move |_| tiptap::toggle_italic()
            >
                "I"
            </button>
            <button
                class="px-2 py-0.5 text-xs underline text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="下划线 (⌘U)"
                on:click=move |_| tiptap::toggle_underline()
            >
                "U"
            </button>

            <span class="text-slate-300 dark:text-slate-600 mx-1">"|"</span>

            // Alignment
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="左对齐"
                on:click=move |_| tiptap::set_text_align("left")
            >
                "左"
            </button>
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="居中对齐"
                on:click=move |_| tiptap::set_text_align("center")
            >
                "中"
            </button>
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="右对齐"
                on:click=move |_| tiptap::set_text_align("right")
            >
                "右"
            </button>

            <span class="text-slate-300 dark:text-slate-600 mx-1">"|"</span>

            // Lists & Quote
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="无序列表"
                on:click=move |_| tiptap::toggle_bullet_list()
            >
                "• 列表"
            </button>
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="有序列表"
                on:click=move |_| tiptap::toggle_ordered_list()
            >
                "1. 列表"
            </button>
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="引用块"
                on:click=move |_| tiptap::toggle_blockquote()
            >
                "“ 引用"
            </button>
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="分割线"
                on:click=move |_| tiptap::insert_horizontal_rule()
            >
                "— 分隔"
            </button>

            <span class="text-slate-300 dark:text-slate-600 mx-1">"|"</span>

            // 插图（PRD：编辑器图片通过本地资产库保存，随 WebDAV 增量备份）
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer disabled:opacity-40"
                title="插入图片（保存到本地资产库，随 WebDAV 增量备份）"
                disabled=move || is_uploading.get()
                on:click=on_insert_image
            >
                {move || if is_uploading.get() { "⏳ 处理中" } else { "🖼 图片" }}
            </button>

            // 上传结果提示（短暂显示，不占常驻空间）
            {move || {
                let msg = upload_msg.get();
                if msg.is_empty() {
                    view! { <span></span> }.into_any()
                } else if msg.starts_with('✓') {
                    view! {
                        <span class="text-[10px] text-emerald-600 dark:text-emerald-400 font-medium">
                            {msg}
                        </span>
                    }.into_any()
                } else {
                    view! {
                        <span class="text-[10px] text-rose-600 dark:text-rose-400 font-medium">
                            {msg}
                        </span>
                    }.into_any()
                }
            }}

            <span class="text-slate-300 dark:text-slate-600 mx-1">"|"</span>

            // Undo / Redo
            <button
                class="px-2 py-0.5 text-xs text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="撤销 (⌘Z)"
                on:click=move |_| tiptap::undo()
            >
                "↶ 撤销"
            </button>
            <button
                class="px-2 py-0.5 text-xs text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="重做 (⌘⇧Z)"
                on:click=move |_| tiptap::redo()
            >
                "↷ 重做"
            </button>
        </div>
    }
}

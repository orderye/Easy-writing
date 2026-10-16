use crate::bridge::tiptap;
use crate::components::editor::image_upload::{pick_and_save_image, UploadEvent};
use crate::state::AppContext;
use crate::tauri_api::invoke_no_args;
use leptos::prelude::*;

#[component]
pub fn EditorToolbar(app_ctx: AppContext) -> impl IntoView {
    let is_uploading = RwSignal::new(false);
    let upload_msg = RwSignal::new(String::new());

    // 插图：选择文件 → 读 base64 → 落库为 asset → 插入 asset:// URL
    //
    // 上传流程本身已抽到 components/editor/image_upload.rs，与图库共用一份；
    // 这里只负责「插入光标处」与进度呈现。
    let on_insert_image = move |_| {
        if is_uploading.get() {
            return;
        }
        pick_and_save_image(
            app_ctx,
            move |ev| match ev {
                UploadEvent::Started => {
                    is_uploading.set(true);
                    upload_msg.set("正在保存图片...".to_string());
                }
                UploadEvent::Failed(msg) => {
                    upload_msg.set(msg);
                    is_uploading.set(false);
                }
            },
            move |rel_path, name| {
                leptos::task::spawn_local(async move {
                    // 相对路径 → asset:// URL，插入编辑器
                    let app_dir = invoke_no_args::<String>("get_app_data_dir")
                        .await
                        .unwrap_or_default();
                    let url = crate::bridge::asset::resolve_asset_url(&rel_path, &app_dir);
                    tiptap::insert_image(&url, &name);
                    app_ctx.save_status.set("dirty".to_string());
                    upload_msg.set("✓ 图片已插入".to_string());
                    is_uploading.set(false);
                });
            },
        );
    };

    // 打开图库：复用已入库的图片，并可见「文件缺失」情况
    let on_open_gallery = move |_| app_ctx.is_asset_gallery_open.set(true);

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

            // 图库：浏览本作品已入库的图片，并插回光标处
            <button
                class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                title="打开图库（已入库图片：插入、体检、统计）"
                on:click=on_open_gallery
            >
                "🗂 图库"
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
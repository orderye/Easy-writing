use crate::bridge::tiptap;
use crate::components::editor::image_upload::{pick_and_save_image, UploadEvent};
use crate::components::icons::{IconFolder, IconImage};
use crate::state::AppContext;
use leptos::prelude::*;

#[component]
pub fn EditorToolbar(app_ctx: AppContext) -> impl IntoView {
    let is_uploading = RwSignal::new(false);
    let upload_msg = RwSignal::new(String::new());

    // 插图：选择文件 → 读 base64 → 落库为 asset → 插入 asset:// URL
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
                let canonical_src = format!("asset://local/{}", rel_path);
                tiptap::insert_image(&canonical_src, &name);
                app_ctx.save_status.set("dirty".to_string());
                upload_msg.set("✓ 图片已插入".to_string());
                is_uploading.set(false);
            },
        );
    };

    let on_open_gallery = move |_| app_ctx.is_asset_gallery_open.set(true);

    let has_chapter = move || app_ctx.current_chapter.get().is_some();

    view! {
        <div class=move || {
            let base_classes = "h-9 border-b border-slate-200/80 dark:border-slate-800 bg-slate-50/90 dark:bg-slate-900/90 backdrop-blur-md px-3 flex items-center gap-1 select-none z-10 shrink-0 overflow-x-auto";
            if has_chapter() {
                base_classes.to_string()
            } else {
                format!("{base_classes} opacity-40 pointer-events-none")
            }
        }>
            // Headings
            <div class="flex items-center gap-0.5 bg-slate-200/50 dark:bg-slate-800/60 p-0.5 rounded-lg border border-slate-200/50 dark:border-slate-700/50">
                <button
                    class="px-2 py-0.5 text-xs font-semibold text-slate-700 dark:text-slate-200 hover:bg-white dark:hover:bg-slate-700 hover:shadow-xs rounded-md transition-all cursor-pointer"
                    title="一级标题"
                    on:click=move |_| tiptap::toggle_heading(1)
                >
                    "H1"
                </button>
                <button
                    class="px-2 py-0.5 text-xs font-semibold text-slate-700 dark:text-slate-200 hover:bg-white dark:hover:bg-slate-700 hover:shadow-xs rounded-md transition-all cursor-pointer"
                    title="二级标题"
                    on:click=move |_| tiptap::toggle_heading(2)
                >
                    "H2"
                </button>
                <button
                    class="px-2 py-0.5 text-xs font-semibold text-slate-700 dark:text-slate-200 hover:bg-white dark:hover:bg-slate-700 hover:shadow-xs rounded-md transition-all cursor-pointer"
                    title="三级标题"
                    on:click=move |_| tiptap::toggle_heading(3)
                >
                    "H3"
                </button>
            </div>

            <span class="w-px h-4 bg-slate-200 dark:bg-slate-700 mx-1"></span>

            // Inline styles
            <div class="flex items-center gap-0.5">
                <button
                    class="w-6 h-6 flex items-center justify-center text-xs font-bold text-slate-700 dark:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="加粗 (⌘B)"
                    on:click=move |_| tiptap::toggle_bold()
                >
                    "B"
                </button>
                <button
                    class="w-6 h-6 flex items-center justify-center text-xs italic font-serif text-slate-700 dark:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="斜体 (⌘I)"
                    on:click=move |_| tiptap::toggle_italic()
                >
                    "I"
                </button>
                <button
                    class="w-6 h-6 flex items-center justify-center text-xs underline text-slate-700 dark:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="下划线 (⌘U)"
                    on:click=move |_| tiptap::toggle_underline()
                >
                    "U"
                </button>
            </div>

            <span class="w-px h-4 bg-slate-200 dark:bg-slate-700 mx-1"></span>

            // Alignment
            <div class="flex items-center gap-0.5">
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="左对齐"
                    on:click=move |_| tiptap::set_text_align("left")
                >
                    "左"
                </button>
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="居中对齐"
                    on:click=move |_| tiptap::set_text_align("center")
                >
                    "中"
                </button>
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="右对齐"
                    on:click=move |_| tiptap::set_text_align("right")
                >
                    "右"
                </button>
            </div>

            <span class="w-px h-4 bg-slate-200 dark:bg-slate-700 mx-1"></span>

            // Lists & Quote
            <div class="flex items-center gap-0.5">
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="无序列表"
                    on:click=move |_| tiptap::toggle_bullet_list()
                >
                    "• 列表"
                </button>
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="有序列表"
                    on:click=move |_| tiptap::toggle_ordered_list()
                >
                    "1. 列表"
                </button>
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="引用块"
                    on:click=move |_| tiptap::toggle_blockquote()
                >
                    "“ 引用"
                </button>
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="分割线"
                    on:click=move |_| tiptap::insert_horizontal_rule()
                >
                    "— 分隔"
                </button>
            </div>

            <span class="w-px h-4 bg-slate-200 dark:bg-slate-700 mx-1"></span>

            // 插图与图库
            <div class="flex items-center gap-1">
                <button
                    class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-40"
                    title="插入图片（保存到本地资产库，随 WebDAV 增量备份）"
                    disabled=move || is_uploading.get()
                    on:click=on_insert_image
                >
                    <IconImage class="w-3.5 h-3.5 text-slate-500 dark:text-slate-400" />
                    <span>{move || if is_uploading.get() { "处理中" } else { "图片" }}</span>
                </button>

                <button
                    class="px-2 py-0.5 text-xs text-slate-700 dark:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer flex items-center gap-1.5"
                    title="打开图库（已入库图片：插入、体检、统计）"
                    on:click=on_open_gallery
                >
                    <IconFolder class="w-3.5 h-3.5 text-slate-500 dark:text-slate-400" />
                    <span>"图库"</span>
                </button>
            </div>

            // 上传结果提示
            {move || {
                let msg = upload_msg.get();
                if msg.is_empty() {
                    view! { <span></span> }.into_any()
                } else if msg.starts_with('✓') {
                    view! {
                        <span class="text-[11px] text-emerald-600 dark:text-emerald-400 font-medium px-1.5 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/40">
                            {msg}
                        </span>
                    }.into_any()
                } else {
                    view! {
                        <span class="text-[11px] text-rose-600 dark:text-rose-400 font-medium px-1.5 py-0.5 rounded bg-rose-50 dark:bg-rose-950/40">
                            {msg}
                        </span>
                    }.into_any()
                }
            }}

            <div class="flex-1"></div>

            // Undo / Redo
            <div class="flex items-center gap-0.5">
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="撤销 (⌘Z)"
                    on:click=move |_| tiptap::undo()
                >
                    "撤销"
                </button>
                <button
                    class="px-1.5 py-0.5 text-xs text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 hover:bg-slate-200/70 dark:hover:bg-slate-700 rounded-md transition-colors cursor-pointer"
                    title="重做 (⌘⇧Z)"
                    on:click=move |_| tiptap::redo()
                >
                    "重做"
                </button>
            </div>
        </div>
    }
}

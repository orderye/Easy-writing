use crate::bridge::tiptap;
use leptos::prelude::*;

#[component]
pub fn EditorToolbar() -> impl IntoView {
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

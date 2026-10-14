use crate::state::AppContext;
use leptos::prelude::*;

#[component]
pub fn StatusBar(app_ctx: AppContext) -> impl IntoView {
    let word_count = move || app_ctx.word_count.get();
    let status_text = move || match app_ctx.save_status.get().as_str() {
        "dirty" => ("● 有未保存修改", "text-amber-500 dark:text-amber-400"),
        "saving" => ("↻ 正在保存...", "text-blue-500 dark:text-blue-400"),
        "error" => ("✕ 保存失败", "text-red-500 dark:text-red-400"),
        _ => ("✓ 已保存", "text-emerald-500 dark:text-emerald-400"),
    };

    view! {
        <div class="h-7 border-t border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 px-4 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 select-none z-10 shrink-0">
            <div class="flex items-center gap-4">
                <span>"本章字数: " <strong class="text-slate-700 dark:text-slate-200">{word_count}</strong> " 字"</span>
                <span class="text-slate-300 dark:text-slate-600">"|"</span>
                <span class="flex items-center gap-1.5 font-medium">
                    <span class=move || status_text().1>{move || status_text().0}</span>
                </span>
            </div>

            <div class="flex items-center gap-3 text-slate-400 dark:text-slate-500">
                <button
                    class="hover:text-blue-600 dark:hover:text-blue-400 cursor-pointer flex items-center gap-1 transition-colors"
                    title="查看章节版本历史与时间轴回滚"
                    on:click=move |_| {
                        if app_ctx.current_chapter.get().is_some() {
                            app_ctx.is_revisions_modal_open.set(true);
                        }
                    }
                >
                    <span>"🕒"</span>
                    <span>"版本历史"</span>
                </button>
                <span class="text-slate-300 dark:text-slate-600">"|"</span>
                <span>"快捷键: ⌘/Ctrl+S 保存"</span>
                <span class="text-slate-300 dark:text-slate-600">"|"</span>
                <span>"自动保存已开启"</span>
            </div>
        </div>
    }
}

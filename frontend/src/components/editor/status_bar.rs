use crate::state::AppContext;
use leptos::prelude::*;

#[component]
pub fn StatusBar(app_ctx: AppContext) -> impl IntoView {
    let word_count = move || app_ctx.word_count.get();
    let status_text = move || match app_ctx.save_status.get().as_str() {
        "dirty" => ("● 有未保存修改", "text-amber-500"),
        "saving" => ("↻ 正在保存...", "text-blue-500"),
        "error" => ("✕ 保存失败", "text-red-500"),
        _ => ("✓ 已保存", "text-emerald-500"),
    };

    view! {
        <div class="h-7 border-t border-slate-200 bg-slate-50 px-4 flex items-center justify-between text-xs text-slate-500 select-none z-10 shrink-0">
            <div class="flex items-center gap-4">
                <span>"本章字数: " <strong class="text-slate-700">{word_count}</strong> " 字"</span>
                <span class="text-slate-300">"|"</span>
                <span class="flex items-center gap-1.5 font-medium">
                    <span class=move || status_text().1>{move || status_text().0}</span>
                </span>
            </div>

            <div class="flex items-center gap-3 text-slate-400">
                <span>"快捷键: ⌘S 保存"</span>
                <span class="text-slate-300">"|"</span>
                <span>"自动保存已开启"</span>
            </div>
        </div>
    }
}

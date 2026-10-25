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
                // 自动保存连续失败时的常驻告警：只在按钮上变红太容易被忽略，
                // 磁盘满/库被锁这类故障必须让用户看得见。
                {move || {
                    app_ctx
                        .save_alert
                        .get()
                        .map(|msg| {
                            view! {
                                <span class="flex items-center gap-1 font-semibold text-red-600 dark:text-red-400">
                                    <span>"⚠"</span>
                                    <span>{msg}</span>
                                </span>
                            }
                        })
                }}
            </div>

            <div class="flex items-center gap-3 text-slate-400 dark:text-slate-500">
                // 每日目标进度（任务书 T11）：显示「今日 X 字」，设了目标再补比例。
                {move || {
                    let net = app_ctx.today_net_words.get();
                    let goal = app_ctx.daily_goal.get();
                    if goal > 0 {
                        let pct = ((net as f64 / goal as f64) * 100.0).round() as i64;
                        let pct = pct.clamp(0, 999);
                        view! {
                            <span class="flex items-center gap-1">
                                <span>{format!("今日 {net}/{goal} 字 ({pct}%)")}</span>
                            </span>
                            <span class="text-slate-300 dark:text-slate-600">"|"</span>
                        }.into_any()
                    } else {
                        view! {
                            <span>{format!("今日 {net} 字")}</span>
                            <span class="text-slate-300 dark:text-slate-600">"|"</span>
                        }.into_any()
                    }
                }}
                <button
                    class=move || {
                        if app_ctx.is_ghost_completion_enabled.get() {
                            "hover:text-blue-600 dark:hover:text-blue-400 cursor-pointer flex items-center gap-1 transition-colors text-blue-600 dark:text-blue-400 font-medium"
                        } else {
                            "hover:text-slate-600 dark:hover:text-slate-300 cursor-pointer flex items-center gap-1 transition-colors text-slate-400"
                        }
                    }
                    title="点击切换：小说词库实时灰字预测补全 (按 Tab 键一键确认)"
                    on:click=move |_| {
                        app_ctx.is_ghost_completion_enabled.update(|v| *v = !*v);
                    }
                >
                    <span>"⚡"</span>
                    <span>
                        {move || {
                            if app_ctx.is_ghost_completion_enabled.get() {
                                "词库补全: 开 (Tab确认)"
                            } else {
                                "词库补全: 关"
                            }
                        }}
                    </span>
                </button>
                <span class="text-slate-300 dark:text-slate-600">"|"</span>
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
                <button
                    class=move || {
                        if app_ctx.is_focus_mode.get() || app_ctx.is_typewriter_mode.get() {
                            "hover:text-blue-600 dark:hover:text-blue-400 cursor-pointer flex items-center gap-1 transition-colors text-blue-600 dark:text-blue-400 font-medium"
                        } else {
                            "hover:text-blue-600 dark:hover:text-blue-400 cursor-pointer flex items-center gap-1 transition-colors text-slate-400"
                        }
                    }
                    title=move || {
                        if app_ctx.is_focus_mode.get() && app_ctx.is_typewriter_mode.get() {
                            "专注模式 + 打字机模式已开启（⌘/Ctrl+Shift+F 两栏，⌘/Ctrl+Shift+T 打字机）"
                        } else if app_ctx.is_focus_mode.get() {
                            "专注模式已开启，隐藏左右两栏（⌘/Ctrl+Shift+F 退出；⌘/Ctrl+Shift+T 打字机模式）"
                        } else if app_ctx.is_typewriter_mode.get() {
                            "打字机模式已开启，光标行居中（⌘/Ctrl+Shift+T 退出）"
                        } else {
                            "专注模式：隐藏左右两栏（⌘/Ctrl+Shift+F）；打字机模式：光标行居中（⌘/Ctrl+Shift+T）"
                        }
                    }
                    on:click=move |_| {
                        app_ctx.is_focus_mode.update(|v| *v = !*v);
                    }
                >
                    <span>"🎯"</span>
                    <span>
                        {move || {
                            if app_ctx.is_focus_mode.get() {
                                if app_ctx.is_typewriter_mode.get() {
                                    "专注 + 打字机"
                                } else {
                                    "专注模式: 开"
                                }
                            } else if app_ctx.is_typewriter_mode.get() {
                                "打字机模式: 开"
                            } else {
                                "专注模式: 关"
                            }
                        }}
                    </span>
                </button>
                <span class="text-slate-300 dark:text-slate-600">"|"</span>
                <span>"快捷键: ⌘/Ctrl+S 保存"</span>
                <span class="text-slate-300 dark:text-slate-600">"|"</span>
                <span>"自动保存已开启"</span>
            </div>
        </div>
    }
}

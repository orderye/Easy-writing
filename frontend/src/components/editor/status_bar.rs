use crate::components::icons::{
    IconAlertTriangle, IconCheck, IconHistory, IconMaximize, IconRefresh, IconZap,
};
use crate::state::AppContext;
use leptos::prelude::*;

#[component]
pub fn StatusBar(app_ctx: AppContext) -> impl IntoView {
    let word_count = move || app_ctx.word_count.get();
    
    // 阅读时间预估（按中文平均 400 字/分钟）
    let reading_time = move || {
        let count = word_count();
        if count == 0 {
            0
        } else {
            (count as f64 / 400.0).ceil() as u32
        }
    };

    let status_view = move || match app_ctx.save_status.get().as_str() {
        "dirty" => view! {
            <span class="flex items-center gap-1.5 text-amber-500 dark:text-amber-400 font-medium">
                <span class="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse"></span>
                <span>"有未保存修改"</span>
            </span>
        }.into_any(),
        "saving" => view! {
            <span class="flex items-center gap-1.5 text-blue-500 dark:text-blue-400 font-medium">
                <IconRefresh class="w-3 h-3 animate-spin" />
                <span>"正在保存..."</span>
            </span>
        }.into_any(),
        "error" => view! {
            <span class="flex items-center gap-1.5 text-rose-500 dark:text-rose-400 font-medium">
                <IconAlertTriangle class="w-3 h-3" />
                <span>"保存失败"</span>
            </span>
        }.into_any(),
        _ => view! {
            <span class="flex items-center gap-1.5 text-emerald-500 dark:text-emerald-400 font-medium">
                <IconCheck class="w-3 h-3" />
                <span>"已保存"</span>
            </span>
        }.into_any(),
    };

    view! {
        <div class="h-7 border-t border-slate-200/80 dark:border-slate-800 bg-slate-50/90 dark:bg-slate-900/90 backdrop-blur-md px-4 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 select-none z-10 shrink-0">
            <div class="flex items-center gap-4">
                <span class="flex items-center gap-1">
                    <span>"本章字数: "</span>
                    <strong class="font-semibold text-slate-700 dark:text-slate-200">{word_count}</strong>
                    <span>" 字"</span>
                    <span class="text-[11px] text-slate-400 dark:text-slate-500 ml-1">
                        {move || format!("(约 {} 分钟读完)", reading_time())}
                    </span>
                </span>
                <span class="text-slate-300 dark:text-slate-700">"|"</span>
                <span class="flex items-center gap-1.5">
                    {status_view}
                </span>

                // 自动保存连续失败时的常驻告警
                {move || {
                    app_ctx
                        .save_alert
                        .get()
                        .map(|msg| {
                            view! {
                                <span class="flex items-center gap-1 font-semibold text-rose-600 dark:text-rose-400 bg-rose-50 dark:bg-rose-950/40 px-2 py-0.5 rounded">
                                    <IconAlertTriangle class="w-3.5 h-3.5" />
                                    <span>{msg}</span>
                                </span>
                            }
                        })
                }}
            </div>

            <div class="flex items-center gap-3 text-slate-400 dark:text-slate-500">
                // 每日目标进度
                {move || {
                    let net = app_ctx.today_net_words.get();
                    let goal = app_ctx.daily_goal.get();
                    if goal > 0 {
                        let pct = ((net as f64 / goal as f64) * 100.0).round() as i64;
                        let pct = pct.clamp(0, 999);
                        view! {
                            <span class="flex items-center gap-1.5 font-medium text-slate-600 dark:text-slate-300">
                                <span>{format!("今日 {net}/{goal} 字 ({pct}%)")}</span>
                            </span>
                            <span class="text-slate-300 dark:text-slate-700">"|"</span>
                        }.into_any()
                    } else {
                        view! {
                            <span class="font-medium text-slate-600 dark:text-slate-300">
                                {format!("今日 {net} 字")}
                            </span>
                            <span class="text-slate-300 dark:text-slate-700">"|"</span>
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
                    <IconZap class="w-3.5 h-3.5" />
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

                <span class="text-slate-300 dark:text-slate-700">"|"</span>

                <button
                    class="hover:text-blue-600 dark:hover:text-blue-400 cursor-pointer flex items-center gap-1 transition-colors text-slate-500 dark:text-slate-400"
                    title="查看章节版本历史与时间轴回滚"
                    on:click=move |_| {
                        if app_ctx.current_chapter.get().is_some() {
                            app_ctx.is_revisions_modal_open.set(true);
                        }
                    }
                >
                    <IconHistory class="w-3.5 h-3.5" />
                    <span>"版本历史"</span>
                </button>

                <span class="text-slate-300 dark:text-slate-700">"|"</span>

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
                    <IconMaximize class="w-3.5 h-3.5" />
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

                <span class="text-slate-300 dark:text-slate-700">"|"</span>
                <span class="text-[11px] text-slate-400">"⌘S 保存"</span>
            </div>
        </div>
    }
}

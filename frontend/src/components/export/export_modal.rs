//! 作品导出弹层（任务书 T02）。
//!
//! 作者在这里选格式与范围 → 选好保存位置 → 后台导出 → 看进度 → 拿结果。
//!
//! ## 为什么进度走事件而不是等命令返回
//!
//! 导出是「重活」：300 章 / 百万字要读库、解析正文、写文件。
//! 命令层把它丢进 `spawn_blocking`，前端在等待期间必须还能画画、
//! 还能取消 —— 所以进度由后端用事件推过来，命令本身等到导出结束才返回。
//!
//! ## 为什么目标路径由系统对话框决定
//!
//! 导出文件是作者要拿去投稿或发布的，落到哪儿得他自己选。
//! 应用替他猜一个目录，猜错了就是「文件去哪了」的问题。

use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{
    ChapterHeader, ExportFormat, ExportOptions, ExportProgress, ExportSummary, ParagraphSeparator,
};
use std::collections::HashSet;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlInputElement;

/// 进度事件名（与后端 `commands::export_cmd::PROGRESS_EVENT` 一致）。
const PROGRESS_EVENT: &str = "novel-export://progress";

#[component]
pub fn ExportModal(app_ctx: AppContext) -> impl IntoView {
    let format = RwSignal::new("txt".to_string());
    let scope_mode = RwSignal::new("all".to_string()); // "all" | "volumes" | "chapters"
    let selected_volumes = RwSignal::new(HashSet::<String>::new());
    let selected_chapters = RwSignal::new(HashSet::<String>::new());
    let volume_template = RwSignal::new("{volume_index} {volume_title}".to_string());
    let chapter_template = RwSignal::new("{chapter_index} {chapter_title}".to_string());
    let separator = RwSignal::new("indent".to_string()); // "indent" | "blank_line"
    let include_images = RwSignal::new(false);
    let target_path = RwSignal::new(Option::<String>::None);
    // 全书章头：选章范围要用。
    let chapter_headers = RwSignal::new(Vec::<ChapterHeader>::new());

    // 进度订阅：整个窗口只订一次。
    //
    // 放在 `on_mount` 而不是组件体里 —— 组件体在每次响应式更新时都会重跑，
    // 在那里订阅会越订越多。
    Effect::new(move |_| {
        let ctx = app_ctx;
        crate::bridge::events::listen_with(PROGRESS_EVENT, move |value: JsValue| {
            match serde_wasm_bindgen::from_value::<ExportProgress>(value) {
                Ok(progress) => ctx.export_progress.set(Some(progress)),
                Err(err) => {
                    ctx.export_error
                        .set(Some(format!("导出进度解析失败: {}", err)));
                }
            }
        });
    });

    // 打开弹层时拉一次全书章头。
    //
    // 只有「按章导出」才用得到它，但提前拉好可以让作者在弹层里直接勾选，
    // 不用等切换模式后才开始加载。
    Effect::new(move |_| {
        if !app_ctx.is_export_modal_open.get() {
            return;
        }
        let Some(novel) = app_ctx.current_novel.get() else {
            return;
        };
        let novel_id = novel.id;
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("list_all_chapter_headers_for_novel")
                .arg("novel_id", novel_id)
                .send::<Vec<ChapterHeader>>()
                .await;
            if let Ok(headers) = res {
                chapter_headers.set(headers);
            }
        });
    });

    // 选保存位置。
    let on_choose_path = move |_| {
        let Some(novel) = app_ctx.current_novel.get() else {
            app_ctx
                .export_error
                .set(Some("没有打开的作品，无法导出。".to_string()));
            return;
        };
        let file_name = if novel.title.trim().is_empty() {
            "未命名作品".to_string()
        } else {
            novel.title.clone()
        };
        let fmt = format.get();
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("choose_export_file")
                .arg("file_name", file_name)
                .arg("format", fmt)
                .send::<Option<String>>()
                .await;
            match res {
                // 作者取消了系统对话框 —— 这不是错误，不该弹红字。
                Ok(None) => {}
                Ok(Some(path)) => {
                    app_ctx.export_error.set(None);
                    target_path.set(Some(path));
                }
                Err(e) => app_ctx.export_error.set(Some(e)),
            }
        });
    };

    // 开始导出。
    let on_start = move |_| {
        let Some(novel) = app_ctx.current_novel.get() else {
            app_ctx
                .export_error
                .set(Some("没有打开的作品，无法导出。".to_string()));
            return;
        };
        let Some(path) = target_path.get() else {
            app_ctx
                .export_error
                .set(Some("请先选择保存位置。".to_string()));
            return;
        };

        let format_value = match format.get().as_str() {
            "docx" => ExportFormat::Docx,
            "epub" => ExportFormat::Epub,
            _ => ExportFormat::Txt,
        };
        let separator_value = match separator.get().as_str() {
            "blank_line" => ParagraphSeparator::BlankLine,
            _ => ParagraphSeparator::Indent,
        };

        // 范围：整书 / 指定卷 / 指定章。
        let (volume_ids, chapter_ids) = match scope_mode.get().as_str() {
            "volumes" => (
                selected_volumes.get().into_iter().collect::<Vec<_>>(),
                Vec::new(),
            ),
            "chapters" => (
                Vec::new(),
                selected_chapters.get().into_iter().collect::<Vec<_>>(),
            ),
            _ => (Vec::new(), Vec::new()),
        };

        let options = ExportOptions {
            novel_id: novel.id,
            format: format_value,
            volume_ids,
            chapter_ids,
            volume_title_template: volume_template.get(),
            chapter_title_template: chapter_template.get(),
            paragraph_separator: separator_value,
            include_images: include_images.get(),
            file_name: novel.title,
        };

        app_ctx.export_error.set(None);
        app_ctx.export_progress.set(None);
        app_ctx.export_summary.set(None);
        app_ctx.export_running.set(true);

        let ctx = app_ctx;
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("export_novel")
                .arg("path", path)
                .arg("options", options)
                .send::<ExportSummary>()
                .await;
            ctx.export_running.set(false);
            match res {
                Ok(summary) => ctx.export_summary.set(Some(summary)),
                Err(e) => ctx.export_error.set(Some(e)),
            }
        });
    };

    // 取消导出。
    let on_cancel = move |_| {
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("cancel_export").send_unit().await;
        });
    };

    // 关闭并清空一次性状态。
    let on_close = move |_| {
        app_ctx.is_export_modal_open.set(false);
        app_ctx.export_error.set(None);
        app_ctx.export_progress.set(None);
        app_ctx.export_summary.set(None);
        target_path.set(None);
    };

    view! {
        {move || {
            if !app_ctx.is_export_modal_open.get() {
                return view! { <div></div> }.into_any();
            }
            let running = app_ctx.export_running.get();
            let progress = app_ctx.export_progress.get();
            let summary = app_ctx.export_summary.get();
            let error = app_ctx.export_error.get();
            // 读一次这几个信号，建立响应式依赖：
            // 否则作者在弹层里改格式 / 范围 / 排版，界面不会跟着变。
            let _ = format.get();
            let _ = scope_mode.get();
            let _ = separator.get();
            let _ = include_images.get();
            let chapters = chapter_headers.get();
            let volumes_now = app_ctx.volumes_list.get();
            let selected_volumes_now = selected_volumes.get();
            let selected_chapters_now = selected_chapters.get();

            view! {
                <div
                    class="fixed inset-0 z-[60] flex items-center justify-center bg-slate-900/50 p-4"
                    on:click=move |_| {
                        // 导出进行中不关闭：关了弹层就看不到进度，也没法取消。
                        if !app_ctx.export_running.get() {
                            on_close(());
                        }
                    }
                >
                    <div
                        class="w-full max-w-2xl max-h-[88vh] flex flex-col bg-white dark:bg-slate-900 rounded-xl shadow-2xl border border-slate-200 dark:border-slate-700"
                        on:click=|ev| ev.stop_propagation()
                    >
                        // Header
                        <div class="flex items-center justify-between px-5 py-3 border-b border-slate-200 dark:border-slate-800 shrink-0">
                            <div class="flex items-center gap-2">
                                <span class="text-base">"📤"</span>
                                <h3 class="text-sm font-bold text-slate-800 dark:text-slate-100">"导出作品"</h3>
                            </div>
                            <button
                                type="button"
                                class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 cursor-pointer text-lg leading-none"
                                disabled=running
                                on:click=move |_| on_close(())
                            >
                                "×"
                            </button>
                        </div>

                        // Body
                        <div class="flex-1 overflow-y-auto px-5 py-4 space-y-4 text-xs">
                            // 格式
                            <div class="space-y-1.5">
                                <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"导出格式"</label>
                                <div class="flex items-center gap-2">
                                    {[
                                        ("txt", "纯文本 (.txt)"),
                                        ("docx", "Word 文档 (.docx)"),
                                        ("epub", "电子书 (.epub)"),
                                    ]
                                    .into_iter()
                                    .map(|(key, label)| {
                                        let key_static = key.to_string();
                                        view! {
                                            <button
                                                type="button"
                                                class=if format.get() == key_static {
                                                    "px-3 py-1.5 rounded-md text-xs font-semibold bg-blue-600 text-white shadow-xs cursor-pointer"
                                                } else {
                                                    "px-3 py-1.5 rounded-md text-xs font-medium bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 cursor-pointer"
                                                }
                                                on:click=move |_| format.set(key_static.clone())
                                            >
                                                {label}
                                            </button>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                                </div>
                            </div>

                            // 范围
                            <div class="space-y-1.5">
                                <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"导出范围"</label>
                                <div class="flex items-center gap-3 text-[11px] text-slate-600 dark:text-slate-300">
                                    <label class="flex items-center gap-1 cursor-pointer">
                                        <input
                                            type="radio"
                                            name="export_scope"
                                            checked=move || scope_mode.get() == "all"
                                            on:change=move |_| scope_mode.set("all".to_string())
                                        />
                                        <span>"整部作品"</span>
                                    </label>
                                    <label class="flex items-center gap-1 cursor-pointer">
                                        <input
                                            type="radio"
                                            name="export_scope"
                                            checked=move || scope_mode.get() == "volumes"
                                            on:change=move |_| scope_mode.set("volumes".to_string())
                                        />
                                        <span>"按卷选择"</span>
                                    </label>
                                    <label class="flex items-center gap-1 cursor-pointer">
                                        <input
                                            type="radio"
                                            name="export_scope"
                                            checked=move || scope_mode.get() == "chapters"
                                            on:change=move |_| scope_mode.set("chapters".to_string())
                                        />
                                        <span>"按章选择"</span>
                                    </label>
                                </div>

                                {move || {
                                    if scope_mode.get() == "volumes" {
                                        let volumes = volumes_now.clone();
                                        let selected = selected_volumes_now.clone();
                                        view! {
                                            <div class="max-h-40 overflow-y-auto border border-slate-200 dark:border-slate-700 rounded p-2 space-y-1">
                                                {volumes.into_iter().map(|vol| {
                                                    let vol_id = vol.id.clone();
                                                    let checked = selected.contains(&vol_id);
                                                    view! {
                                                        <label class="flex items-center gap-2 cursor-pointer text-[11px] text-slate-600 dark:text-slate-300">
                                                            <input
                                                                type="checkbox"
                                                                checked=move || checked
                                                                on:change=move |_| {
                                                                    selected_volumes.update(|set| {
                                                                        if set.contains(&vol_id) {
                                                                            set.remove(&vol_id);
                                                                        } else {
                                                                            set.insert(vol_id.clone());
                                                                        }
                                                                    });
                                                                }
                                                            />
                                                            <span>{vol.title.clone()}</span>
                                                        </label>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    } else if scope_mode.get() == "chapters" {
                                        let chapters = chapters.clone();
                                        let selected = selected_chapters_now.clone();
                                        view! {
                                            <div class="max-h-40 overflow-y-auto border border-slate-200 dark:border-slate-700 rounded p-2 space-y-1">
                                                {chapters.into_iter().map(|ch| {
                                                    let ch_id = ch.id.clone();
                                                    let checked = selected.contains(&ch_id);
                                                    view! {
                                                        <label class="flex items-center gap-2 cursor-pointer text-[11px] text-slate-600 dark:text-slate-300">
                                                            <input
                                                                type="checkbox"
                                                                checked=move || checked
                                                                on:change=move |_| {
                                                                    selected_chapters.update(|set| {
                                                                        if set.contains(&ch_id) {
                                                                            set.remove(&ch_id);
                                                                        } else {
                                                                            set.insert(ch_id.clone());
                                                                        }
                                                                    });
                                                                }
                                                            />
                                                            <span class="truncate">{ch.title.clone()}</span>
                                                        </label>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }
                                }}
                            </div>

                            // 标题模板
                            <div class="grid grid-cols-2 gap-3">
                                <div class="space-y-1.5">
                                    <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"卷标题模板"</label>
                                    <input
                                        type="text"
                                        class="w-full text-xs px-2.5 py-1.5 border border-slate-300 dark:border-slate-600 rounded bg-slate-50 dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                        prop:value=move || volume_template.get()
                                        on:input=move |ev| volume_template.set(event_target_value(&ev))
                                    />
                                    <p class="text-[10px] text-slate-400">"占位符：{novel} {volume_index} {volume_title}"</p>
                                </div>
                                <div class="space-y-1.5">
                                    <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"章标题模板"</label>
                                    <input
                                        type="text"
                                        class="w-full text-xs px-2.5 py-1.5 border border-slate-300 dark:border-slate-600 rounded bg-slate-50 dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                        prop:value=move || chapter_template.get()
                                        on:input=move |ev| chapter_template.set(event_target_value(&ev))
                                    />
                                    <p class="text-[10px] text-slate-400">"占位符：{chapter_index} {chapter_title}"</p>
                                </div>
                            </div>

                            // 排版
                            <div class="grid grid-cols-2 gap-3">
                                <div class="space-y-1.5">
                                    <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"段落排版"</label>
                                    <div class="flex items-center gap-3 text-[11px] text-slate-600 dark:text-slate-300">
                                        <label class="flex items-center gap-1 cursor-pointer">
                                            <input
                                                type="radio"
                                                name="export_sep"
                                                checked=move || separator.get() == "indent"
                                                on:change=move |_| separator.set("indent".to_string())
                                            />
                                            <span>"段首缩进两字"</span>
                                        </label>
                                        <label class="flex items-center gap-1 cursor-pointer">
                                            <input
                                                type="radio"
                                                name="export_sep"
                                                checked=move || separator.get() == "blank_line"
                                                on:change=move |_| separator.set("blank_line".to_string())
                                            />
                                            <span>"段间空一行"</span>
                                        </label>
                                    </div>
                                </div>
                                <div class="space-y-1.5">
                                    <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"图片"</label>
                                    <label class="flex items-center gap-1.5 text-[11px] text-slate-600 dark:text-slate-300 cursor-pointer">
                                        <input
                                            type="checkbox"
                                            checked=move || include_images.get()
                                            disabled=move || format.get() == "txt"
                                            on:change=move |ev| {
                                                let checked = ev
                                                    .target()
                                                    .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
                                                    .map(|input| input.checked())
                                                    .unwrap_or(false);
                                                include_images.set(checked);
                                            }
                                        />
                                        <span>
                                            {if format.get() == "txt" {
                                                "纯文本不支持图片"
                                            } else {
                                                "包含正文中的图片"
                                            }}
                                        </span>
                                    </label>
                                </div>
                            </div>

                            // 目标文件
                            <div class="space-y-1.5">
                                <label class="text-xs font-semibold text-slate-700 dark:text-slate-200">"保存位置"</label>
                                <div class="flex items-center gap-2">
                                    <div class="flex-1 text-[11px] px-2.5 py-1.5 border border-slate-200 dark:border-slate-700 rounded bg-slate-50 dark:bg-slate-900 text-slate-600 dark:text-slate-300 truncate">
                                        {move || match target_path.get() {
                                            Some(path) => path,
                                            None => "尚未选择文件".to_string(),
                                        }}
                                    </div>
                                    <button
                                        type="button"
                                        class="px-3 py-1.5 text-xs bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-750 text-slate-700 dark:text-slate-200 rounded-md font-medium cursor-pointer shrink-0"
                                        disabled=running
                                        on:click=move |_| on_choose_path(())
                                    >
                                        "选择位置…"
                                    </button>
                                </div>
                            </div>

                            // 进度
                            {move || {
                                match progress.clone() {
                                    Some(p) => {
                                        let percent = if p.total == 0 {
                                            0
                                        } else {
                                            (p.done as f64 * 100.0 / p.total as f64) as u32
                                        };
                                        view! {
                                            <div class="space-y-1.5">
                                                <div class="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
                                                    <span>
                                                        {p.done} " / " {p.total} " 章"
                                                    </span>
                                                    <span class="truncate max-w-[240px]">{p.current_title.clone()}</span>
                                                </div>
                                                <div class="h-1.5 w-full rounded-full bg-slate-200 dark:bg-slate-700 overflow-hidden">
                                                    <div
                                                        class="h-full bg-blue-600 transition-all"
                                                        style=format!("width: {}%", percent)
                                                    ></div>
                                                </div>
                                            </div>
                                        }.into_any()
                                    }
                                    None => view! { <div></div> }.into_any(),
                                }
                            }}

                            // 错误
                            {move || match error.clone() {
                                Some(e) => {
                                    view! {
                                        <div class="px-3 py-2 rounded-md bg-red-50 dark:bg-red-950/50 border border-red-200 dark:border-red-800 text-[11px] text-red-700 dark:text-red-300">
                                            {e.clone()}
                                        </div>
                                    }.into_any()
                                }
                                None => view! { <div></div> }.into_any(),
                            }}

                            // 结果
                            {move || match summary.clone() {
                                Some(s) => {
                                    view! {
                                        <div class="px-3 py-2.5 rounded-md bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 text-[11px] text-emerald-800 dark:text-emerald-300 space-y-1">
                                            {if s.cancelled {
                                                view! {
                                                    <div>"已取消导出，未留下半成品文件。"</div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="font-semibold">"导出完成"</div>
                                                }.into_any()
                                            }}
                                            <div>
                                                {s.volume_count} " 卷 · " {s.chapter_count} " 章 · " {s.word_count} " 字"
                                            </div>
                                            <div>
                                                "图片：" {s.included_images} " 张"
                                                {if s.skipped_images > 0 {
                                                    view! { <span>"（跳过 " {s.skipped_images} " 张）"</span> }.into_any()
                                                } else {
                                                    view! { <span></span> }.into_any()
                                                }}
                                            </div>
                                            {if !s.degraded_chapters.is_empty() {
                                                view! {
                                                    <div class="text-amber-700 dark:text-amber-300">
                                                        "以下章节正文不是富文本格式，已按纯文本导出："
                                                        {s.degraded_chapters.join("、")}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div></div> }.into_any()
                                            }}
                                            {if !s.path.is_empty() {
                                                view! {
                                                    <div class="truncate text-emerald-700/80 dark:text-emerald-300/80">
                                                        {s.path.clone()}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div></div> }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                }
                                None => view! { <div></div> }.into_any(),
                            }}
                        </div>

                        // Footer
                        <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-850 flex items-center justify-between shrink-0">
                            <span class="text-[11px] text-slate-400">
                                "导出在后台执行，界面不会卡住"
                            </span>
                            <div class="flex items-center gap-2">
                                {move || if running {
                                    view! {
                                        <div class="flex items-center gap-2">
                                            <button
                                                type="button"
                                                class="px-3.5 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 rounded-md font-medium transition-colors cursor-pointer"
                                                on:click=move |_| on_cancel(())
                                            >
                                                "取消导出"
                                            </button>
                                            <button
                                                type="button"
                                                class="px-4 py-1.5 text-xs bg-blue-600/60 text-white rounded-md font-semibold shadow-xs transition-colors cursor-not-allowed"
                                                disabled=true
                                            >
                                                "导出中…"
                                            </button>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="flex items-center gap-2">
                                            <button
                                                type="button"
                                                class="px-3.5 py-1.5 text-xs text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-750 rounded-md font-medium transition-colors cursor-pointer"
                                                on:click=move |_| on_close(())
                                            >
                                                "关闭"
                                            </button>
                                            <button
                                                type="button"
                                                class="px-4 py-1.5 text-xs bg-emerald-600 hover:bg-emerald-700 text-white rounded-md font-semibold shadow-xs transition-colors cursor-pointer flex items-center gap-1.5 disabled:opacity-50"
                                                disabled=move || target_path.get().is_none()
                                                on:click=move |_| on_start(())
                                            >
                                                "开始导出"
                                            </button>
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

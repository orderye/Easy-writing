//! 全书搜索与替换弹层（任务书 T09）。
//!
//! ## 交互结构
//!
//! 两个 tab：
//! - **搜索**：输入即查（300ms 防抖），结果按章列出，点一条跳到那一章
//!   （走 `request_switch_chapter`，未保存拦截自动生效）；
//! - **替换**：查找 + 替换为 → 预览（逐章命中数，默认全选）→ 执行 →
//!   结果摘要 + 「撤销本次替换」。
//!
//! ## 为什么替换前必须预览
//!
//! 批量替换是唯一会一次改动多章正文的操作。预览把「会改什么」摆在明面上，
//! 作者取消勾选个别章再执行；执行本身仍受快照 + 整批撤销兜底。

use crate::components::structure_panel::chapter_list::request_switch_chapter;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{ChapterHeader, ChapterSearchHit, ReplacePreviewItem, ReplaceReport, UndoReplaceReport};
use web_sys::HtmlInputElement;

/// 搜索防抖间隔（毫秒）。
const DEBOUNCE_MS: u32 = 300;

#[component]
pub fn SearchModal(app_ctx: AppContext) -> impl IntoView {
    let is_open = app_ctx.is_search_open;
    let tab = RwSignal::new("search".to_string()); // "search" | "replace"
    let query = RwSignal::new(String::new());
    let replacement = RwSignal::new(String::new());
    let case_sensitive = RwSignal::new(false);

    let hits = RwSignal::new(Vec::<ChapterSearchHit>::new());
    let previews = RwSignal::new(Vec::<ReplacePreviewItem>::new());
    let selected = RwSignal::new(std::collections::HashSet::<String>::new());
    let is_searching = RwSignal::new(false);
    let is_applying = RwSignal::new(false);
    let message = RwSignal::new(Option::<String>::None);
    let last_batch = RwSignal::new(Option::<String>::None);
    let searched_query = RwSignal::new(String::new());

    // 关闭弹层时清状态，下次打开是干净的面板。
    Effect::new(move |_| {
        if !is_open.get() {
            tab.set("search".to_string());
            query.set(String::new());
            replacement.set(String::new());
            hits.set(Vec::new());
            previews.set(Vec::new());
            selected.set(std::collections::HashSet::new());
            message.set(None);
            last_batch.set(None);
            searched_query.set(String::new());
        }
    });

    // 搜索：300ms 防抖后调用后端。
    Effect::new(move |_| {
        let q = query.get();
        let novel_id = app_ctx.current_novel.get_untracked().map(|n| n.id);
        let Some(novel_id) = novel_id else {
            hits.set(Vec::new());
            return;
        };
        if q.trim().is_empty() {
            hits.set(Vec::new());
            searched_query.set(String::new());
            return;
        }
        let timer = gloo_timers::future::TimeoutFuture::new(DEBOUNCE_MS);
        leptos::task::spawn_local(async move {
            timer.await;
            // 防抖期间输入又变了：让最新的那次 Effect 来发请求。
            if query.get_untracked() != q {
                return;
            }
            is_searching.set(true);
            let res = invoke_cmd("search_chapters")
                .arg("novel_id", novel_id)
                .arg("query", q.clone())
                .opt("limit", Some(50u32))
                .send::<Vec<ChapterSearchHit>>()
                .await;
            match res {
                Ok(list) => {
                    searched_query.set(q.trim().to_string());
                    hits.set(list);
                }
                Err(e) => {
                    hits.set(Vec::new());
                    message.set(Some(format!("搜索失败：{}", e)));
                }
            }
            is_searching.set(false);
        });
    });

    let run_preview = move || {
        let Some(novel) = app_ctx.current_novel.get_untracked() else {
            return;
        };
        let q = query.get_untracked().trim().to_string();
        if q.is_empty() {
            return;
        }
        let case = case_sensitive.get_untracked();
        leptos::task::spawn_local(async move {
            is_searching.set(true);
            match invoke_cmd("preview_replace")
                .arg("novel_id", novel.id)
                .arg("query", q)
                .opt("case_sensitive", Some(case))
                .send::<Vec<ReplacePreviewItem>>()
                .await
            {
                Ok(items) => {
                    selected.set(items.iter().map(|i| i.chapter_id.clone()).collect());
                    previews.set(items);
                }
                Err(e) => message.set(Some(format!("预览失败：{}", e))),
            }
            is_searching.set(false);
        });
    };

    let run_apply = move || {
        let Some(novel) = app_ctx.current_novel.get_untracked() else {
            return;
        };
        let q = query.get_untracked().trim().to_string();
        let rep = replacement.get_untracked();
        if q.is_empty() {
            message.set(Some("查找内容不能为空".to_string()));
            return;
        }
        let ids: Vec<String> = selected.get_untracked().into_iter().collect();
        if ids.is_empty() {
            message.set(Some("请至少勾选一个章节".to_string()));
            return;
        }
        let case = case_sensitive.get_untracked();
        leptos::task::spawn_local(async move {
            is_applying.set(true);
            match invoke_cmd("apply_replace")
                .arg("novel_id", novel.id)
                .arg("query", q)
                .arg("replacement", rep)
                .opt("chapter_ids", Some(ids))
                .opt("case_sensitive", Some(case))
                .send::<ReplaceReport>()
                .await
            {
                Ok(report) => {
                    last_batch.set(Some(report.batch_id.clone()));
                    message.set(Some(format!(
                        "已替换 {} 章，共 {} 处{}",
                        report.chapters_changed,
                        report.total_replacements,
                        if report.skipped_chapters.is_empty() {
                            String::new()
                        } else {
                            format!("；跳过 {} 章（正文格式不支持）", report.skipped_chapters.len())
                        }
                    )));
                    previews.set(Vec::new());
                }
                Err(e) => message.set(Some(format!("替换失败：{}", e))),
            }
            is_applying.set(false);
        });
    };

    let run_undo = move || {
        let Some(batch) = last_batch.get_untracked() else {
            return;
        };
        leptos::task::spawn_local(async move {
            match invoke_cmd("undo_replace_batch")
                .arg("batch_id", batch)
                .send::<UndoReplaceReport>()
                .await
            {
                Ok(report) => {
                    last_batch.set(None);
                    message.set(Some(format!(
                        "已还原 {} 章{}",
                        report.chapters_restored,
                        if report.missing_chapters.is_empty() {
                            String::new()
                        } else {
                            format!("；{} 章未能还原", report.missing_chapters.len())
                        }
                    )));
                }
                Err(e) => message.set(Some(format!("撤销失败：{}", e))),
            }
        });
    };

    // 点搜索结果跳章：复用结构面板的切章逻辑（未保存拦截、竞态保护都在里面）。
    let jump_to = move |chapter_id: String| {
        let header = app_ctx
            .chapters_map
            .get_untracked()
            .into_iter()
            .find(|h| h.id == chapter_id);
        if let Some(h) = header {
            is_open.set(false);
            request_switch_chapter(app_ctx, h);
        }
    };

    view! {
        {move || if !is_open.get() { view! { <span class="hidden"></span> }.into_any() } else {
            view! {
                <div class="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24"
                    on:click=move |_| is_open.set(false)
                >
                    <div class="w-[640px] max-w-[92vw] bg-white dark:bg-slate-900 rounded-xl shadow-2xl border border-slate-200 dark:border-slate-700 overflow-hidden"
                        on:click=move |ev| ev.stop_propagation()
                    >
                        // Tab 切换
                        <div class="flex border-b border-slate-200 dark:border-slate-700 text-sm">
                            <button
                                class=move || tab_button_cls(&tab.get(), "search")
                                on:click=move |_| tab.set("search".to_string())
                            >"搜索"</button>
                            <button
                                class=move || tab_button_cls(&tab.get(), "replace")
                                on:click=move |_| { tab.set("replace".to_string()); }
                            >"替换"</button>
                            <button class="ml-auto px-4 text-slate-400 hover:text-slate-600 cursor-pointer"
                                on:click=move |_| is_open.set(false)
                            >"✕"</button>
                        </div>

                        <div class="p-4 space-y-3">
                            <div class="flex gap-2">
                                <input
                                    type="text"
                                    class="flex-1 px-3 py-1.5 text-sm bg-white dark:bg-slate-850 border border-slate-200 dark:border-slate-700 rounded-md text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                    placeholder="在全书正文中查找…"
                                    prop:value=move || query.get()
                                    on:input=move |ev| query.set(event_target_value(&ev))
                                />
                                {move || if tab.get() == "replace" {
                                    view! {
                                        <input
                                            type="text"
                                            class="flex-1 px-3 py-1.5 text-sm bg-white dark:bg-slate-850 border border-slate-200 dark:border-slate-700 rounded-md text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                            placeholder="替换为…"
                                            prop:value=move || replacement.get()
                                            on:input=move |ev| replacement.set(event_target_value(&ev))
                                        />
                                    }.into_any()
                                } else { view! { <span></span> }.into_any() }}
                            </div>

                            <label class="flex items-center gap-1.5 text-xs text-slate-500 cursor-pointer select-none">
                                <input
                                    type="checkbox"
                                    class="w-3.5 h-3.5 text-blue-600 rounded cursor-pointer"
                                    prop:checked=move || case_sensitive.get()
                                    on:change=move |_| case_sensitive.update(|v| *v = !*v)
                                />
                                "区分大小写"
                            </label>

                            {move || match tab.get().as_str() {
                                "replace" => view! {
                                    <div class="flex gap-2">
                                        <button
                                            class="px-3 py-1.5 text-sm rounded-md bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 cursor-pointer"
                                            disabled=move || is_searching.get() || query.get().trim().is_empty()
                                            on:click=move |_| run_preview()
                                        >"预览替换"</button>
                                        {move || if !previews.get().is_empty() {
                                            view! {
                                                <button
                                                    class="px-3 py-1.5 text-sm rounded-md bg-emerald-600 text-white hover:bg-emerald-700 disabled:opacity-50 cursor-pointer"
                                                    disabled=move || is_applying.get()
                                                    on:click=move |_| run_apply()
                                                >"执行替换"</button>
                                            }.into_any()
                                        } else { view! { <span></span> }.into_any() }}
                                    </div>
                                }.into_any(),
                                _ => {
                                    let _ = &searched_query;
                                    view! { <span></span> }.into_any()
                                }
                            }}

                            {move || message.get().map(|m| view! {
                                <div class="text-xs px-3 py-2 rounded-md bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-200">
                                    {m}
                                    {move || if last_batch.get().is_some() {
                                        view! {
                                            <button class="ml-2 text-blue-600 hover:underline cursor-pointer"
                                                on:click=move |_| run_undo()
                                            >"撤销本次替换"</button>
                                        }.into_any()
                                    } else { view! { <span></span> }.into_any() }}
                                </div>
                            }).into_any()}

                            // 搜索结果 / 替换预览共用的列表区
                            {move || {
                                let in_replace_tab = tab.get() == "replace";
                                let items: Vec<(String, String, i64, Vec<String>, bool)> = if in_replace_tab {
                                    previews.get().iter().map(|p| (
                                        p.chapter_id.clone(), p.title.clone(), p.hits,
                                        p.snippets.clone(), p.unsupported,
                                    )).collect()
                                } else {
                                    hits.get().iter().map(|h| (
                                        h.chapter_id.clone(), h.title.clone(), h.hits,
                                        h.snippets.clone(), false,
                                    )).collect()
                                };
                                if items.is_empty() {
                                    let hint = if in_replace_tab {
                                        "输入查找内容后点「预览替换」".to_string()
                                    } else if query.get().trim().is_empty() {
                                        String::new()
                                    } else if is_searching.get() {
                                        "搜索中…".to_string()
                                    } else {
                                        "没有找到匹配的章节".to_string()
                                    };
                                    return view! {
                                        <div class="text-xs text-slate-400 py-6 text-center">{hint}</div>
                                    }.into_any();
                                }
                                let total: i64 = items.iter().map(|(_, _, h, _, _)| *h).sum();
                                let chapter_count = items.len();
                                view! {
                                    <div class="space-y-1">
                                        <div class="text-[11px] text-slate-400 pb-1">
                                            {format!("{} 章，共 {} 处命中", chapter_count, total)}
                                            {move || if !in_replace_tab && searched_query.get().chars().count() < 3 {
                                                view! {
                                                    <span class="ml-2 text-amber-600">"短词走字面匹配（全文索引要求 ≥3 字符）"</span>
                                                }.into_any()
                                            } else { view! { <span></span> }.into_any() }}
                                        </div>
                                        <For
                                            each=move || items.clone()
                                            key=|item| item.0.clone()
                                            children=move |item| {
                                                let (chapter_id, title, hits_count, snippets, unsupported) = item;
                                                let title_for_click = title.clone();
                                                let id_for_click = chapter_id.clone();
                                                // 勾选态与切换各自需要一个 owned id：闭包只能 move 一次。
                                                let id_for_checked = chapter_id.clone();
                                                let id_for_toggle = chapter_id.clone();
                                                let checked = move || selected.get().contains(&id_for_checked);
                                                view! {
                                                    <div class="flex items-start gap-2 px-2 py-1.5 rounded hover:bg-slate-50 dark:hover:bg-slate-800/60">
                                                        {if in_replace_tab {
                                                            view! {
                                                                <input
                                                                    type="checkbox"
                                                                    class="mt-0.5 w-3.5 h-3.5 text-blue-600 rounded cursor-pointer"
                                                                    prop:checked=checked
                                                                    on:change=move |_| selected.update(|s| {
                                                                        if s.contains(&id_for_toggle) { s.remove(&id_for_toggle); } else { s.insert(id_for_toggle.clone()); }
                                                                    })
                                                                />
                                                            }.into_any()
                                                        } else { view! { <span></span> }.into_any() }}
                                                        <div class="flex-1 min-w-0">
                                                            <div class="flex items-center gap-2">
                                                                <button class="text-xs font-medium text-slate-800 dark:text-slate-100 hover:text-blue-600 cursor-pointer truncate max-w-[60%] text-left"
                                                                    title=move || title_for_click.clone()
                                                                    on:click=move |_| jump_to(id_for_click.clone())
                                                                >{title}</button>
                                                                {if unsupported {
                                                                    view! { <span class="text-[9px] px-1 rounded bg-amber-100 text-amber-700">"格式不支持，将跳过"</span> }.into_any()
                                                                } else { view! { <span></span> }.into_any() }}
                                                                <span class="text-[10px] text-slate-400 font-mono ml-auto shrink-0">
                                                                    {format!("{} 处", hits_count)}
                                                                </span>
                                                            </div>
                                                            <div class="space-y-0.5">
                                                                {snippets.into_iter().map(|s| view! {
                                                                    <div class="text-[11px] text-slate-500 truncate">{s}</div>
                                                                }).collect_view()}
                                                            </div>
                                                        </div>
                                                    </div>
                                                }
                                            }
                                        />
                                    </div>
                                }.into_any()
                            }}
                        </div>

                        <div class="px-4 py-2 border-t border-slate-200 dark:border-slate-700 text-[10px] text-slate-400">
                            "替换前会自动为每章建立版本快照，可整批撤销；不足 3 字符的查找词走字面匹配"
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

fn tab_button_cls(current: &str, tab: &str) -> &'static str {
    if current == tab {
        "px-4 py-2 text-blue-600 border-b-2 border-blue-600 font-medium cursor-pointer"
    } else {
        "px-4 py-2 text-slate-500 hover:text-slate-700 cursor-pointer"
    }
}

/// 供测试断言的辅助：命中片段是否包含高亮标记。
#[allow(dead_code)]
fn snippet_has_marks(s: &str) -> bool {
    s.contains('⟦') && s.contains('⟧')
}

// 供 view! 里 node_ref 使用的导入兜底（未用到时保持静默）。
#[allow(unused_imports)]
use leptos::html;

#[allow(dead_code)]
fn _type_anchor(_x: Option<ChapterHeader>) {}
#[allow(dead_code)]
fn _input_ref_anchor(_x: Option<HtmlInputElement>) {}

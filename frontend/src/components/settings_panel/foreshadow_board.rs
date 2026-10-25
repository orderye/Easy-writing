//! 伏笔看板（T10）。
//!
//! 与剧情列表（`plot_lines.rs`）不同：剧情列表是编辑入口，看板是总览。
//! 看板展示每条伏笔的埋设/回收位置、距今章数、超期高亮与关联角色，
//! 并提供一键跳转与标记回收。
//!
//! 「伏笔」口径：展示全部三种 kind（mainline / conflict / foreshadow），
//! 因为 T06 的上下文装配同样把它们都当作「待回收伏笔」带入。

use crate::components::settings_panel::loaders::load_list_for_novel;
use crate::components::structure_panel::chapter_list::request_switch_chapter;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{ChapterHeader, ForeshadowRow};

#[component]
pub fn ForeshadowBoardView(app_ctx: AppContext) -> impl IntoView {
    let board = RwSignal::new(Vec::<ForeshadowRow>::new());

    let load_board = move || {
        load_list_for_novel(app_ctx, "list_foreshadow_board", board);
    };

    // 作品切换 / 看板 tab 激活时加载
    Effect::new(move |_| {
        if app_ctx.active_settings_tab.get() == "foreshadow" && app_ctx.current_novel.get().is_some() {
            load_board();
        }
    });

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
            <div class="flex items-center justify-between text-xs pb-1 border-b border-slate-200 dark:border-slate-800">
                <span class="font-bold text-slate-700 dark:text-slate-300">"伏笔看板"</span>
                <span class="text-slate-400 dark:text-slate-500">
                    {move || {
                        let rows = board.get();
                        let unresolved = rows.iter().filter(|r| r.status == "planted").count();
                        let overdue = rows.iter().filter(|r| r.overdue).count();
                        if rows.is_empty() {
                            "暂无伏笔".to_string()
                        } else {
                            format!("未回收 {} / 超期 {}", unresolved, overdue)
                        }
                    }}
                </span>
            </div>

            {move || {
                let rows = board.get();
                if rows.is_empty() {
                    return view! {
                        <div class="py-8 text-center text-xs text-slate-400">
                            "暂无伏笔。在「剧情」标签页新建，或由 AI 提炼自动生成。"
                        </div>
                    }.into_any();
                }

                // 拥有所有权的 id → 章节表：不借用 chapters_map 的临时 Vec，
                // 每行视图要用的章名与跳转目标都先算成 owned 值。
                let chapter_map: std::collections::HashMap<String, ChapterHeader> = app_ctx
                    .chapters_map
                    .get()
                    .iter()
                    .map(|h| (h.id.clone(), h.clone()))
                    .collect();

                view! {
                    <div class="space-y-2">
                        {rows.into_iter().map(|row| {
                            let r_id = row.id.clone();
                            let r_status = row.status.clone();
                            let r_title = row.title.clone();
                            let r_content = row.content.clone();
                            let r_since = row.chapters_since_plant;
                            let r_overdue = row.overdue;
                            let r_related = row.related_characters.clone();
                            let is_planted = r_status == "planted";

                            let plant_header = row
                                .chapter_id
                                .as_ref()
                                .and_then(|cid| chapter_map.get(cid).cloned());
                            let plant_name = plant_header
                                .as_ref()
                                .map(|h| h.title.clone())
                                .unwrap_or_else(|| "—".to_string());
                            let resolved_name = row
                                .resolved_chapter_id
                                .as_ref()
                                .and_then(|cid| chapter_map.get(cid))
                                .map(|h| h.title.clone())
                                .unwrap_or_else(|| "—".to_string());

                            let card_cls = if r_overdue {
                                "border border-amber-300 dark:border-amber-700 bg-amber-50/40 dark:bg-amber-950/30"
                            } else {
                                "border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900"
                            };

                            let status_badge = if is_planted {
                                view! {
                                    <span class="text-[10px] px-1.5 py-0.5 rounded font-medium bg-amber-100 dark:bg-amber-950/60 text-amber-800 dark:text-amber-300 border border-amber-300 dark:border-amber-700">
                                        "已埋设"
                                    </span>
                                }.into_any()
                            } else {
                                view! {
                                    <span class="text-[10px] px-1.5 py-0.5 rounded font-medium bg-emerald-100 dark:bg-emerald-950/60 text-emerald-800 dark:text-emerald-300 border border-emerald-300 dark:border-emerald-700">
                                        "已回收"
                                    </span>
                                }.into_any()
                            };

                            let overdue_badge = if r_overdue {
                                view! {
                                    <span class="text-[10px] px-1.5 py-0.5 rounded font-medium bg-red-100 dark:bg-red-950/60 text-red-800 dark:text-red-300 border border-red-300 dark:border-red-700">
                                        {format!("⚠ {} 章未回收", r_since.unwrap_or(0))}
                                    </span>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            };

                            let related_chips = if r_related.is_empty() {
                                view! {
                                    <span class="text-[10px] text-slate-400">"未关联角色"</span>
                                }.into_any()
                            } else {
                                r_related.iter().cloned().map(|name| {
                                    view! {
                                        <span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] bg-slate-100 border border-slate-200 text-slate-700">
                                            {name}
                                        </span>
                                    }
                                }).collect_view().into_any()
                            };

                            let jump_button = plant_header
                                .map(|h| {
                                    view! {
                                        <button
                                            class="text-[10px] text-blue-600 dark:text-blue-400 hover:underline"
                                            on:click=move |_| request_switch_chapter(app_ctx, h.clone())
                                        >
                                            "跳转埋设章"
                                        </button>
                                    }.into_any()
                                })
                                .unwrap_or_else(|| view! { <span></span> }.into_any());

                            let resolve_select = if is_planted {
                                let r_id_select = r_id.clone();
                                view! {
                                    <select
                                        class="text-[10px] border border-slate-200 dark:border-slate-700 rounded px-1 py-0.5 bg-white dark:bg-slate-800 text-slate-700"
                                        on:change=move |ev| {
                                            let val = event_target_value(&ev);
                                            if !val.is_empty() {
                                                let cid = val.clone();
                                                let pid = r_id_select.clone();
                                                leptos::task::spawn_local(async move {
                                                    let _ = invoke_cmd("resolve_plot")
                                                        .arg("id", pid)
                                                        .arg("resolved_chapter_id", cid)
                                                        .send_unit()
                                                        .await;
                                                    load_list_for_novel(app_ctx, "list_foreshadow_board", board);
                                                });
                                            }
                                        }
                                    >
                                        <option value="">"选择回收章节…"</option>
                                        {app_ctx.chapters_map.get().iter().map(|h| {
                                            view! {
                                                <option value=h.id.clone()>{h.title.clone()}</option>
                                            }
                                        }).collect_view()}
                                    </select>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            };

                            view! {
                                <div class=format!("rounded-lg p-2.5 shadow-2xs space-y-2 {}", card_cls)>
                                    <div class="flex items-center justify-between gap-2">
                                        <div class="flex items-center gap-1.5 flex-1 min-w-0">
                                            <span class="text-sm">"📌"</span>
                                            <span class="font-semibold text-xs text-slate-800 dark:text-slate-100 truncate">{r_title}</span>
                                        </div>
                                        <div class="flex items-center gap-1.5 shrink-0">
                                            {status_badge}
                                            {overdue_badge}
                                        </div>
                                    </div>

                                    <div class="text-[11px] text-slate-500 dark:text-slate-400 space-y-1">
                                        <div class="flex items-center gap-2">
                                            <span class="text-slate-400">"埋设："</span>
                                            <span>{plant_name}</span>
                                            {if let Some(n) = r_since {
                                                view! {
                                                    <span class="text-slate-400">"距今 "</span> {n} " 章"
                                                }.into_any()
                                            } else {
                                                view! { <span class="text-slate-400">"（无埋设章节）"</span> }.into_any()
                                            }}
                                        </div>
                                        {if is_planted {
                                            view! { <div></div> }.into_any()
                                        } else {
                                            view! {
                                                <div class="flex items-center gap-2">
                                                    <span class="text-slate-400">"回收："</span>
                                                    <span>{resolved_name}</span>
                                                </div>
                                            }.into_any()
                                        }}
                                    </div>

                                    <div class="flex flex-wrap gap-1">{related_chips}</div>

                                    <div class="text-[11px] text-slate-600 dark:text-slate-400 leading-relaxed line-clamp-2">
                                        {r_content}
                                    </div>

                                    <div class="flex items-center justify-between gap-2 pt-1">
                                        <div class="flex items-center gap-1.5">{jump_button}</div>
                                        {resolve_select}
                                    </div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

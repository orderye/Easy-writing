pub mod chapter_list;
pub mod save_button;
pub mod volume_list;

use crate::components::ai::polish_button::PolishButton;
use crate::components::layout::panel_toggle::RightToggle;
use crate::components::structure_panel::save_button::SaveButton;
use crate::components::structure_panel::volume_list::VolumeList;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;

#[component]
pub fn StructurePanel(app_ctx: AppContext) -> impl IntoView {
    let current_chapter_summary = RwSignal::new(String::new());
    let current_chapter_status = RwSignal::new("draft".to_string());
    // 当前章节的 POV / 故事时间草稿（任务书 T11）：select 的 on:change 拿不到
    // 自身的最新值，用信号中转。
    let current_pov_id = RwSignal::new(String::new());
    let current_story_time = RwSignal::new(String::new());
    // POV 筛选下拉的角色候选（任务书 T11）：面板打开时按当前作品拉一次。
    let pov_candidates = RwSignal::new(Vec::<shared_types::Character>::new());

    // Sync from current_chapter signal
    Effect::new(move |_| {
        if let Some(ch) = app_ctx.current_chapter.get() {
            current_chapter_summary.set(ch.summary);
            current_chapter_status.set(ch.status);
            current_pov_id.set(ch.pov_character_id.clone().unwrap_or_default());
            current_story_time.set(ch.story_time.clone().unwrap_or_default());
        }
    });

    // 拉取角色候选（POV 下拉用）。切作品时刷新。
    Effect::new(move |_| {
        let novel_id = app_ctx.current_novel.get().map(|n| n.id);
        leptos::task::spawn_local(async move {
            if let Some(nid) = novel_id {
                if let Ok(list) = invoke_cmd("list_characters")
                    .arg("novel_id", nid)
                    .send::<Vec<shared_types::Character>>()
                    .await
                {
                    pov_candidates.set(list);
                }
            }
        });
    });

    // Save summary and status changes
    let save_chapter_meta = move |new_summary: String, new_status: String| {
        if let Some(mut ch) = app_ctx.current_chapter.get() {
            if ch.summary != new_summary || ch.status != new_status {
                ch.summary = new_summary.clone();
                ch.status = new_status.clone();
                app_ctx.current_chapter.set(Some(ch.clone()));

                // Update chapters_map for SSOT
                let mut map = app_ctx.chapters_map.get();
                if let Some(item) = map.iter_mut().find(|h| h.id == ch.id) {
                    item.summary = new_summary.clone();
                    item.status = new_status.clone();
                }
                app_ctx.chapters_map.set(map);

                let cid = ch.id;
                let ctitle = ch.title;
                leptos::task::spawn_local(async move {
                    let _ = invoke_cmd("update_chapter_meta")
                        .arg("id", cid)
                        .arg("title", ctitle)
                        .arg("summary", new_summary)
                        .arg("status", new_status)
                        .send_unit()
                        .await;
                });
            }
        }
    };

    // Save POV / story_time changes（任务书 T11）
    let save_chapter_pov = move |pov: String, story_time: String| {
        let Some(mut ch) = app_ctx.current_chapter.get() else { return };
        let pov_opt = if pov.is_empty() { None } else { Some(pov.clone()) };
        let time_opt = if story_time.trim().is_empty() {
            None
        } else {
            Some(story_time.trim().to_string())
        };
        if ch.pov_character_id == pov_opt && ch.story_time == time_opt {
            return;
        }
        ch.pov_character_id = pov_opt.clone();
        ch.story_time = time_opt.clone();
        app_ctx.current_chapter.set(Some(ch.clone()));

        let mut map = app_ctx.chapters_map.get();
        if let Some(item) = map.iter_mut().find(|h| h.id == ch.id) {
            item.pov_character_id = pov_opt.clone();
            item.story_time = time_opt.clone();
        }
        app_ctx.chapters_map.set(map);

        let cid = ch.id;
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("update_chapter_pov")
                .arg("id", cid)
                .opt("pov_character_id", pov_opt)
                .opt("story_time", time_opt)
                .send_unit()
                .await;
        });
    };

    Effect::new(move |_| {
        let s = current_chapter_summary.get();
        let st = current_chapter_status.get();
        save_chapter_meta(s, st);
    });

    view! {
        <aside
            class=move || {
                format!(
                    "h-full bg-slate-50 dark:bg-slate-900 border-l border-slate-200 dark:border-slate-800 flex flex-col panel-transition select-none shrink-0 {}",
                    if app_ctx.right_panel_open.get() && !app_ctx.is_focus_mode.get() { "w-72" } else { "w-0 overflow-hidden border-none" }
                )
            }
        >
            <div class="h-12 border-b border-slate-200 dark:border-slate-700 px-3 flex items-center justify-between shrink-0">
                <span class="font-bold text-slate-800 dark:text-slate-100 text-sm">"作品结构"</span>
                <RightToggle is_open=app_ctx.right_panel_open />
            </div>

            <div class="p-3 border-b border-slate-200 dark:border-slate-700 shrink-0">
                <SaveButton app_ctx=app_ctx />
            </div>
            // Chapter Search & Status Filter
            <div class="px-3 pt-2 pb-2.5 border-b border-slate-200 dark:border-slate-800 space-y-2 shrink-0 bg-slate-50/50 dark:bg-slate-900/50">
                <div class="relative flex items-center">
                    <span class="absolute left-2 text-slate-400 text-xs pointer-events-none select-none">"🔍"</span>
                    <input
                        type="text"
                        class="w-full text-xs pl-6 pr-6 py-1 bg-white dark:bg-slate-850 border border-slate-200 dark:border-slate-700 rounded-md text-slate-800 dark:text-slate-100 placeholder-slate-400 focus:outline-hidden focus:ring-1 focus:ring-blue-500 transition-colors"
                        placeholder="检索章节标题..."
                        prop:value=move || app_ctx.chapter_search_query.get()
                        on:input=move |ev| app_ctx.chapter_search_query.set(event_target_value(&ev))
                    />
                    {move || {
                        if !app_ctx.chapter_search_query.get().is_empty() {
                            view! {
                                <button
                                    class="absolute right-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 text-xs cursor-pointer select-none"
                                    title="清空搜索"
                                    on:click=move |_| app_ctx.chapter_search_query.set(String::new())
                                >
                                    "✕"
                                </button>
                            }.into_any()
                        } else {
                            view! { <span class="hidden"></span> }.into_any()
                        }
                    }}
                </div>

                // Status Filter Pills
                <div class="flex items-center gap-1 text-[11px]">
                    {[
                        ("all", "全部"),
                        ("draft", "草稿"),
                        ("editing", "修改中"),
                        ("finalized", "定稿"),
                    ].into_iter().map(|(val, label)| {
                        let v_str = val.to_string();
                        let is_selected = move || app_ctx.chapter_status_filter.get() == val;
                        view! {
                            <button
                                class=move || {
                                    if is_selected() {
                                        "flex-1 py-0.5 rounded text-center font-medium bg-blue-600 text-white shadow-2xs transition-colors cursor-pointer"
                                    } else {
                                        "flex-1 py-0.5 rounded text-center text-slate-500 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-slate-800 transition-colors cursor-pointer"
                                    }
                                }
                                on:click=move |_| app_ctx.chapter_status_filter.set(v_str.clone())
                            >
                                {label}
                            </button>
                        }
                    }).collect::<Vec<_>>()}
                </div>

                // POV / 故事时间筛选（任务书 T11）
                <div class="flex items-center gap-1.5">
                    <select
                        class="flex-1 min-w-0 text-[11px] bg-white dark:bg-slate-850 border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 rounded px-1 py-0.5 focus:outline-blue-500 cursor-pointer truncate"
                        title="按视角角色筛选章节"
                        prop:value=move || app_ctx.chapter_pov_filter.get()
                        on:change=move |ev| app_ctx.chapter_pov_filter.set(event_target_value(&ev))
                    >
                        <option value="">"视角：全部"</option>
                        {move || {
                            pov_candidates.get().into_iter().map(|c| {
                                view! {
                                    <option value={c.id.clone()}>{c.name.clone()}</option>
                                }
                            }).collect::<Vec<_>>()
                        }}
                    </select>
                    <input
                        type="text"
                        class="flex-1 min-w-0 text-[11px] px-1.5 py-0.5 bg-white dark:bg-slate-850 border border-slate-200 dark:border-slate-700 rounded-md text-slate-800 dark:text-slate-100 placeholder-slate-400 focus:outline-hidden focus:ring-1 focus:ring-blue-500 transition-colors"
                        placeholder="故事时间…"
                        title="按故事内时间筛选（子串匹配，如「冬」）"
                        prop:value=move || app_ctx.chapter_story_time_filter.get()
                        on:input=move |ev| app_ctx.chapter_story_time_filter.set(event_target_value(&ev))
                    />
                </div>
            </div>

            <VolumeList app_ctx=app_ctx />

            // Current chapter metadata & status card
            {move || {
                if let Some(ch) = app_ctx.current_chapter.get() {
                    view! {
                        <div class="border-t border-slate-200 dark:border-slate-700 p-3 bg-white dark:bg-slate-800 space-y-2 shrink-0">
                            <div class="flex items-center justify-between text-xs">
                                <span class="font-semibold text-slate-700 dark:text-slate-200 truncate max-w-[120px]">
                                    {ch.title.clone()}
                                </span>
                                <select
                                    class="text-[11px] bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded px-1.5 py-0.5 focus:outline-blue-500 font-medium cursor-pointer"
                                    prop:value=move || current_chapter_status.get()
                                    on:change=move |ev| current_chapter_status.set(event_target_value(&ev))
                                >
                                    <option value="draft">"草稿"</option>
                                    <option value="editing">"修改中"</option>
                                    <option value="finalized">"已定稿"</option>
                                </select>
                            </div>

                            <div class="space-y-1">
                                <div class="flex items-center justify-between">
                                    <span class="text-[10px] text-slate-400 dark:text-slate-500 font-medium">"本章梗概摘要（同步章纲）"</span>
                                    <PolishButton module_key="chapter_summary" content=current_chapter_summary label="修饰" />
                                </div>
                                <textarea
                                    class="w-full text-xs text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-900/60 border border-slate-200 dark:border-slate-700 rounded p-1.5 focus:bg-white dark:focus:bg-slate-900 focus:outline-blue-500 min-h-[48px]"
                                    placeholder="输入单章核心摘要..."
                                    prop:value=move || current_chapter_summary.get()
                                    on:input=move |ev| current_chapter_summary.set(event_target_value(&ev))
                                ></textarea>
                            </div>

                            // 视角角色与故事时间（任务书 T11）
                            <div class="flex items-center gap-1.5">
                                <select
                                    class="flex-1 min-w-0 text-[11px] bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded px-1 py-0.5 focus:outline-blue-500 font-medium cursor-pointer truncate"
                                    title="本章视角角色"
                                    prop:value=move || ch.pov_character_id.clone().unwrap_or_default()
                                    on:change=move |ev| save_chapter_pov(event_target_value(&ev), current_story_time.get_untracked())
                                >
                                    <option value="">"视角：未指定"</option>
                                    {move || {
                                        pov_candidates.get().into_iter().map(|c| {
                                            view! {
                                                <option value={c.id.clone()}>{c.name.clone()}</option>
                                            }
                                        }).collect::<Vec<_>>()
                                    }}
                                </select>
                                <input
                                    type="text"
                                    class="flex-1 min-w-0 text-[11px] px-1.5 py-0.5 bg-slate-50 dark:bg-slate-900/60 border border-slate-200 dark:border-slate-700 rounded text-slate-700 dark:text-slate-200 placeholder-slate-400 focus:outline-blue-500"
                                    placeholder="故事时间…"
                                    title="本章故事内时间，如「第三年·冬」"
                                    prop:value=move || current_story_time.get()
                                    on:change=move |ev| save_chapter_pov(current_pov_id.get_untracked(), event_target_value(&ev))
                                />
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </aside>
    }
}

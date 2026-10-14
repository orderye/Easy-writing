use crate::components::structure_panel::chapter_list::ChapterList;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::Volume;

#[derive(Serialize)]
struct CreateVolumeArgs {
    novel_id: String,
    title: String,
    summary: String,
}

#[derive(Serialize)]
struct DeleteVolumeArgs {
    id: String,
}

#[component]
pub fn VolumeList(app_ctx: AppContext) -> impl IntoView {
    let on_create_volume = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<Volume, _>(
                    "create_volume",
                    &CreateVolumeArgs {
                        novel_id,
                        title: "新卷".to_string(),
                        summary: "".to_string(),
                    },
                )
                .await;

                if let Ok(new_vol) = res {
                    let mut list = app_ctx.volumes_list.get();
                    list.push(new_vol);
                    app_ctx.volumes_list.set(list);
                }
            });
        }
    };

    let on_delete_volume = move |v_id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("delete_volume", &DeleteVolumeArgs { id: v_id.clone() }).await;
            let mut list = app_ctx.volumes_list.get();
            list.retain(|v| v.id != v_id);
            app_ctx.volumes_list.set(list);
        });
    };

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-2">
            <div class="flex items-center justify-between px-1 mb-1">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"卷目与章节"</span>
                <button
                    class="text-xs text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-medium px-2 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 transition-colors cursor-pointer"
                    on:click=on_create_volume
                >
                    "+ 新建卷"
                </button>
            </div>

            {move || {
                let volumes = app_ctx.volumes_list.get();
                let all_chapters = app_ctx.chapters_map.get();
                let query = app_ctx.chapter_search_query.get().to_lowercase().trim().to_string();
                let status_filter = app_ctx.chapter_status_filter.get();
                let is_filtering = !query.is_empty() || status_filter != "all";

                if volumes.is_empty() {
                    return view! {
                        <div class="py-8 text-center text-xs text-slate-400">
                            "暂无卷目，点击上方「+ 新建卷」"
                        </div>
                    }.into_any();
                }

                let mut has_any_match = false;
                let volume_views: Vec<_> = volumes.into_iter().filter_map(|vol| {
                    let vol_id = vol.id.clone();
                    let del_id = vol.id.clone();

                    let (matching_count, total_count) = {
                        let mut m_count = 0;
                        let mut t_count = 0;
                        for ch in &all_chapters {
                            if ch.volume_id == vol_id {
                                t_count += 1;
                                let match_q = query.is_empty() || ch.title.to_lowercase().contains(&query);
                                let match_s = status_filter == "all" || ch.status == status_filter;
                                if match_q && match_s {
                                    m_count += 1;
                                }
                            }
                        }
                        (m_count, t_count)
                    };

                    if is_filtering {
                        if matching_count == 0 {
                            return None;
                        }
                        has_any_match = true;
                    }

                    let is_manually_collapsed = RwSignal::new(false);
                    let should_show_body = move || {
                        if is_filtering {
                            true
                        } else {
                            !is_manually_collapsed.get()
                        }
                    };

                    Some(view! {
                        <div class="border border-slate-200 dark:border-slate-800 rounded-lg bg-white dark:bg-slate-900 overflow-hidden shadow-2xs">
                            <div
                                class="px-2.5 py-1.5 bg-slate-50 dark:bg-slate-850 hover:bg-slate-100 dark:hover:bg-slate-800 flex items-center justify-between cursor-pointer border-b border-slate-200/60 dark:border-slate-800 text-xs font-semibold text-slate-800 dark:text-slate-100 select-none group transition-colors"
                                on:click=move |_| {
                                    if !is_filtering {
                                        is_manually_collapsed.update(|c| *c = !*c);
                                    }
                                }
                            >
                                <div class="flex items-center gap-1.5 truncate flex-1 min-w-0">
                                    <span class="text-slate-400 text-[10px]">
                                        {move || if should_show_body() { "▾" } else { "▸" }}
                                    </span>
                                    <span class="truncate">{vol.title.clone()}</span>
                                    <span class="text-[10px] text-slate-400 dark:text-slate-500 font-normal">
                                        {if is_filtering {
                                            format!("({}/{})", matching_count, total_count)
                                        } else {
                                            format!("({})", total_count)
                                        }}
                                    </span>
                                </div>

                                <div class="flex items-center gap-1 shrink-0">
                                    <button
                                        class="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-700 transition-opacity cursor-pointer"
                                        title="删除此卷（移入回收站）"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            on_delete_volume(del_id.clone());
                                        }
                                    >
                                        "✕"
                                    </button>
                                </div>
                            </div>

                            {move || {
                                if should_show_body() {
                                    view! {
                                        <ChapterList app_ctx=app_ctx volume_id=vol_id.clone() />
                                    }.into_any()
                                } else {
                                    view! { <div class="hidden"></div> }.into_any()
                                }
                            }}
                        </div>
                    })
                }).collect();

                if is_filtering && !has_any_match {
                    return view! {
                        <div class="py-8 text-center text-xs text-slate-400">
                            "未找到匹配的章节"
                        </div>
                    }.into_any();
                }

                view! {
                    <div class="space-y-2">
                        {volume_views}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

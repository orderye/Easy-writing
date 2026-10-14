use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::WorldviewEntry;
use std::collections::HashSet;

#[derive(Serialize)]
struct ListWorldviewArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct SaveWorldviewArgs {
    id: Option<String>,
    novel_id: String,
    group_name: String,
    title: String,
    content: String,
}

#[derive(Serialize)]
struct DeleteWorldviewArgs {
    id: String,
}

#[component]
pub fn WorldviewView(app_ctx: AppContext) -> impl IntoView {
    let entries = RwSignal::new(Vec::<WorldviewEntry>::new());
    let collapsed_groups = RwSignal::new(HashSet::<String>::new());

    let load_entries = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                if let Ok(list) =
                    invoke::<Vec<WorldviewEntry>, _>("list_worldview", &ListWorldviewArgs { novel_id }).await
                {
                    entries.set(list);
                }
            });
        }
    };

    Effect::new(move |_| {
        load_entries();
    });

    let on_add_entry_to_group = move |group_name: String| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<WorldviewEntry, _>(
                    "save_worldview",
                    &SaveWorldviewArgs {
                        id: None,
                        novel_id,
                        group_name,
                        title: "新设定条目".to_string(),
                        content: "".to_string(),
                    },
                )
                .await;

                if let Ok(new_entry) = res {
                    let mut list = entries.get();
                    list.push(new_entry);
                    entries.set(list);
                }
            });
        }
    };

    let on_delete_entry = move |id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("delete_worldview", &DeleteWorldviewArgs { id }).await;
            load_entries();
        });
    };

    let toggle_group = move |group: String| {
        let mut set = collapsed_groups.get();
        if set.contains(&group) {
            set.remove(&group);
        } else {
            set.insert(group);
        }
        collapsed_groups.set(set);
    };

    let expand_all_groups = move || {
        collapsed_groups.set(HashSet::new());
    };

    let collapse_all_groups = move |groups: Vec<String>| {
        let set: HashSet<String> = groups.into_iter().collect();
        collapsed_groups.set(set);
    };

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
            // Actions & Group Expand/Collapse
            <div class="flex items-center justify-between text-xs pb-1 border-b border-slate-200 dark:border-slate-800">
                <div class="flex items-center gap-2">
                    <span class="font-bold text-slate-700 dark:text-slate-300">"世界观设定"</span>
                    <button
                        class="text-[11px] text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
                        title="全部展开"
                        on:click=move |_| expand_all_groups()
                    >
                        "展开"
                    </button>
                    <span class="text-slate-300 dark:text-slate-700">"/"</span>
                    <button
                        class="text-[11px] text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
                        title="全部折叠"
                        on:click=move |_| {
                            let list = entries.get();
                            let all_groups = list.into_iter().map(|e| if e.group_name.is_empty() { "通用".to_string() } else { e.group_name }).collect::<Vec<_>>();
                            collapse_all_groups(all_groups);
                        }
                    >
                        "折叠"
                    </button>
                </div>

                <button
                    class="text-xs text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-semibold px-2 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 transition-colors cursor-pointer"
                    on:click=move |_| on_add_entry_to_group("通用".to_string())
                >
                    "+ 新增条目"
                </button>
            </div>

            // Grouped Accordion List with Search Filtering
            {move || {
                let query = app_ctx.settings_search_query.get().to_lowercase().trim().to_string();
                let all_entries = entries.get();

                let filtered = if query.is_empty() {
                    all_entries
                } else {
                    all_entries.into_iter().filter(|e| {
                        e.title.to_lowercase().contains(&query)
                            || e.content.to_lowercase().contains(&query)
                            || e.group_name.to_lowercase().contains(&query)
                    }).collect()
                };

                if filtered.is_empty() {
                    return view! {
                        <div class="py-8 text-center text-xs text-slate-400">
                            {if query.is_empty() { "暂无世界观条目，点击右上角「+ 新增条目」" } else { "未找到匹配的世界观条目" }}
                        </div>
                    }.into_any();
                }

                // Group entries by group_name
                let mut groups_map: Vec<(String, Vec<WorldviewEntry>)> = Vec::new();
                for entry in filtered {
                    let g_name = if entry.group_name.trim().is_empty() {
                        "通用".to_string()
                    } else {
                        entry.group_name.trim().to_string()
                    };

                    if let Some(pos) = groups_map.iter().position(|(k, _)| k == &g_name) {
                        groups_map[pos].1.push(entry);
                    } else {
                        groups_map.push((g_name, vec![entry]));
                    }
                }

                view! {
                    <div class="space-y-3">
                        {groups_map.into_iter().map(|(grp_name, grp_entries)| {
                            let g_key_for_toggle = grp_name.clone();
                            let g_key_for_add = grp_name.clone();
                            let g_key_icon = grp_name.clone();
                            let g_key_body = grp_name.clone();
                            let count = grp_entries.len();

                            view! {
                                <div class="border border-slate-200 dark:border-slate-800 rounded-lg overflow-hidden bg-white dark:bg-slate-900 shadow-2xs">
                                    // Accordion Group Header
                                    <div
                                        class="px-2.5 py-1.5 bg-slate-100/80 dark:bg-slate-800/80 hover:bg-slate-200/60 dark:hover:bg-slate-800 flex items-center justify-between text-xs font-bold text-slate-700 dark:text-slate-200 cursor-pointer select-none transition-colors border-b border-slate-200/60 dark:border-slate-800"
                                        on:click=move |_| toggle_group(g_key_for_toggle.clone())
                                    >
                                        <div class="flex items-center gap-1.5">
                                            <span class="text-slate-400 text-[10px]">
                                                {move || if collapsed_groups.get().contains(&g_key_icon) { "▶" } else { "▼" }}
                                            </span>
                                            <span>{grp_name.clone()}</span>
                                            <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400 font-normal">
                                                {count}
                                            </span>
                                        </div>

                                        <button
                                            class="text-[11px] text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-medium px-1 rounded hover:bg-blue-100/50 dark:hover:bg-blue-900/40 transition-colors"
                                            title="在此分组下新增"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                on_add_entry_to_group(g_key_for_add.clone());
                                            }
                                        >
                                            "+ 加项"
                                        </button>
                                    </div>

                                    // Accordion Body
                                    {move || {
                                        if collapsed_groups.get().contains(&g_key_body) {
                                            view! { <div class="hidden"></div> }.into_any()
                                        } else {
                                            view! {
                                                <div class="p-2 space-y-2.5">
                                                    {grp_entries.clone().into_iter().map(|entry| {
                                                        let e_id = entry.id.clone();
                                                        let del_id = entry.id.clone();
                                                        let group_signal = RwSignal::new(entry.group_name.clone());
                                                        let title = RwSignal::new(entry.title.clone());
                                                        let content = RwSignal::new(entry.content.clone());

                                                        let save_entry = move |new_grp: String, new_title: String, new_content: String| {
                                                            if let Some(novel) = app_ctx.current_novel.get() {
                                                                let nid = novel.id;
                                                                let eid = e_id.clone();
                                                                leptos::task::spawn_local(async move {
                                                                    let _ = invoke::<WorldviewEntry, _>(
                                                                        "save_worldview",
                                                                        &SaveWorldviewArgs {
                                                                            id: Some(eid),
                                                                            novel_id: nid,
                                                                            group_name: new_grp,
                                                                            title: new_title,
                                                                            content: new_content,
                                                                        },
                                                                    )
                                                                    .await;
                                                                });
                                                            }
                                                        };

                                                        Effect::new(move |_| {
                                                            let g = group_signal.get();
                                                            let t = title.get();
                                                            let c = content.get();
                                                            save_entry(g, t, c);
                                                        });

                                                        view! {
                                                            <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-2.5 bg-slate-50/50 dark:bg-slate-850 shadow-2xs space-y-1.5 hover:border-slate-300 dark:hover:border-slate-700 transition-colors">
                                                                <div class="flex items-center justify-between gap-2">
                                                                    <input
                                                                        type="text"
                                                                        class="w-16 text-[11px] px-1 py-0.5 rounded border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-500 dark:text-slate-400 font-medium shrink-0 truncate focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                                                        placeholder="分组"
                                                                        title="修改所属分组"
                                                                        prop:value=move || group_signal.get()
                                                                        on:input=move |ev| group_signal.set(event_target_value(&ev))
                                                                    />
                                                                    <input
                                                                        type="text"
                                                                        class="font-semibold text-xs text-slate-800 dark:text-slate-100 bg-transparent border-b border-transparent hover:border-slate-300 dark:hover:border-slate-700 focus:border-blue-500 focus:outline-hidden flex-1 truncate px-1"
                                                                        prop:value=move || title.get()
                                                                        on:input=move |ev| title.set(event_target_value(&ev))
                                                                    />
                                                                    <div class="flex items-center gap-1 shrink-0">
                                                                        <PolishButton module_key="worldview" content=content />
                                                                        <button
                                                                            class="text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors cursor-pointer"
                                                                            title="删除此条目（移入回收站）"
                                                                            on:click=move |_| on_delete_entry(del_id.clone())
                                                                        >
                                                                            "✕"
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                                <textarea
                                                                    class="w-full text-xs text-slate-600 dark:text-slate-300 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded p-1.5 focus:outline-hidden focus:ring-1 focus:ring-blue-500 min-h-[60px] leading-relaxed font-serif"
                                                                    placeholder="输入世界规则、地理、历史、派系术语……"
                                                                    prop:value=move || content.get()
                                                                    on:input=move |ev| content.set(event_target_value(&ev))
                                                                ></textarea>
                                                            </div>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            }.into_any()
                                        }
                                    }}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

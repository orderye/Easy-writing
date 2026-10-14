use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{Character, CharacterRelation};
use std::collections::HashMap;

#[derive(Serialize)]
struct ListCharacterArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct SaveCharacterArgs {
    id: Option<String>,
    novel_id: String,
    name: String,
    aliases: String,
    profile: String,
}

#[derive(Serialize)]
struct DeleteCharacterArgs {
    id: String,
}

#[derive(Serialize)]
struct ListRelationsArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct SaveRelationArgs {
    id: Option<String>,
    novel_id: String,
    from_char_id: String,
    to_char_id: String,
    relation_name: String,
    description: String,
}

#[derive(Serialize)]
struct DeleteRelationArgs {
    id: String,
}

#[component]
pub fn CharactersView(app_ctx: AppContext) -> impl IntoView {
    let characters = RwSignal::new(Vec::<Character>::new());
    let relations = RwSignal::new(Vec::<CharacterRelation>::new());
    let active_sub_tab = RwSignal::new("cards".to_string()); // "cards" | "relations"

    // New relation form inputs
    let new_from_id = RwSignal::new(String::new());
    let new_to_id = RwSignal::new(String::new());
    let new_rel_name = RwSignal::new(String::new());
    let new_desc = RwSignal::new(String::new());

    let load_characters = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                if let Ok(list) =
                    invoke::<Vec<Character>, _>("list_characters", &ListCharacterArgs { novel_id }).await
                {
                    characters.set(list);
                }
            });
        }
    };

    let load_relations = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                if let Ok(list) =
                    invoke::<Vec<CharacterRelation>, _>("list_character_relations", &ListRelationsArgs { novel_id }).await
                {
                    relations.set(list);
                }
            });
        }
    };

    Effect::new(move |_| {
        load_characters();
        load_relations();
    });

    let on_add_char = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<Character, _>(
                    "save_character",
                    &SaveCharacterArgs {
                        id: None,
                        novel_id,
                        name: "新角色".to_string(),
                        aliases: "[]".to_string(),
                        profile: "".to_string(),
                    },
                )
                .await;

                if let Ok(new_ch) = res {
                    let mut list = characters.get();
                    list.push(new_ch);
                    characters.set(list);
                }
            });
        }
    };

    let on_delete_char = move |id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("delete_character", &DeleteCharacterArgs { id }).await;
            load_characters();
            load_relations();
        });
    };

    let on_create_relation = move |_| {
        let from_id = new_from_id.get();
        let to_id = new_to_id.get();
        let rel_name = new_rel_name.get();
        let desc = new_desc.get();

        if from_id.is_empty() || to_id.is_empty() || rel_name.trim().is_empty() || from_id == to_id {
            return;
        }

        if let Some(novel) = app_ctx.current_novel.get() {
            let nid = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<CharacterRelation, _>(
                    "save_character_relation",
                    &SaveRelationArgs {
                        id: None,
                        novel_id: nid,
                        from_char_id: from_id,
                        to_char_id: to_id,
                        relation_name: rel_name.trim().to_string(),
                        description: desc.trim().to_string(),
                    },
                )
                .await;

                if let Ok(new_rel) = res {
                    let mut list = relations.get();
                    list.push(new_rel);
                    relations.set(list);
                    new_rel_name.set(String::new());
                    new_desc.set(String::new());
                }
            });
        }
    };

    let on_delete_relation = move |rel_id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("delete_character_relation", &DeleteRelationArgs { id: rel_id }).await;
            load_relations();
        });
    };

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
            // Sub-tab switcher: Cards vs Relations Graph
            <div class="flex items-center justify-between text-xs pb-1.5 border-b border-slate-200 dark:border-slate-800">
                <div class="flex items-center bg-slate-100 dark:bg-slate-800 p-0.5 rounded-md text-[11px] font-medium">
                    <button
                        class=move || {
                            format!(
                                "px-2 py-0.5 rounded transition-colors cursor-pointer {}",
                                if active_sub_tab.get() == "cards" {
                                    "bg-white dark:bg-slate-900 font-bold text-blue-600 dark:text-blue-400 shadow-2xs"
                                } else {
                                    "text-slate-500 dark:text-slate-400 hover:text-slate-700"
                                }
                            )
                        }
                        on:click=move |_| active_sub_tab.set("cards".to_string())
                    >
                        "👤 角色卡"
                    </button>
                    <button
                        class=move || {
                            format!(
                                "px-2 py-0.5 rounded transition-colors cursor-pointer {}",
                                if active_sub_tab.get() == "relations" {
                                    "bg-white dark:bg-slate-900 font-bold text-blue-600 dark:text-blue-400 shadow-2xs"
                                } else {
                                    "text-slate-500 dark:text-slate-400 hover:text-slate-700"
                                }
                            )
                        }
                        on:click=move |_| active_sub_tab.set("relations".to_string())
                    >
                        "🕸️ 关系图谱"
                    </button>
                </div>

                {move || if active_sub_tab.get() == "cards" {
                    view! {
                        <button
                            class="text-xs text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-semibold px-2 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 transition-colors cursor-pointer"
                            on:click=on_add_char
                        >
                            "+ 新增角色"
                        </button>
                    }.into_any()
                } else {
                    view! { <span class="text-[10px] text-slate-400">"共 " {move || relations.get().len()} " 条关系"</span> }.into_any()
                }}
            </div>

            // Tab 1: Cards View
            {move || if active_sub_tab.get() == "cards" {
                let query = app_ctx.settings_search_query.get().to_lowercase().trim().to_string();
                let all_chars = characters.get();

                let filtered = if query.is_empty() {
                    all_chars
                } else {
                    all_chars.into_iter().filter(|c| {
                        c.name.to_lowercase().contains(&query)
                            || c.aliases.to_lowercase().contains(&query)
                            || c.profile.to_lowercase().contains(&query)
                    }).collect()
                };

                if filtered.is_empty() {
                    return view! {
                        <div class="py-8 text-center text-xs text-slate-400">
                            {if query.is_empty() { "暂无角色卡，点击右上角「+ 新增角色」" } else { "未找到匹配的角色卡" }}
                        </div>
                    }.into_any();
                }

                view! {
                    <div class="space-y-3">
                        {filtered.into_iter().map(|ch| {
                            let ch_id = ch.id.clone();
                            let del_id = ch.id.clone();
                            let name = RwSignal::new(ch.name.clone());
                            let profile = RwSignal::new(ch.profile.clone());
                            let aliases = RwSignal::new(ch.aliases.clone());

                            let save_char = move |new_name: String, new_profile: String, new_aliases: String| {
                                if let Some(novel) = app_ctx.current_novel.get() {
                                    let nid = novel.id;
                                    let cid = ch_id.clone();
                                    leptos::task::spawn_local(async move {
                                        let _ = invoke::<Character, _>(
                                            "save_character",
                                            &SaveCharacterArgs {
                                                id: Some(cid),
                                                novel_id: nid,
                                                name: new_name,
                                                aliases: new_aliases,
                                                profile: new_profile,
                                            },
                                        )
                                        .await;
                                    });
                                }
                            };

                            Effect::new(move |_| {
                                let n = name.get();
                                let p = profile.get();
                                let a = aliases.get();
                                save_char(n, p, a);
                            });

                            view! {
                                <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-2.5 bg-white dark:bg-slate-900 shadow-2xs space-y-2 hover:border-slate-300 dark:hover:border-slate-700 transition-colors">
                                    <div class="flex items-center justify-between gap-2">
                                        <div class="flex items-center gap-1.5 flex-1 min-w-0">
                                            <span class="text-sm">"👤"</span>
                                            <input
                                                type="text"
                                                class="font-semibold text-xs text-slate-800 dark:text-slate-100 bg-transparent border-b border-transparent hover:border-slate-300 dark:hover:border-slate-700 focus:border-blue-500 focus:outline-hidden flex-1 truncate px-1"
                                                prop:value=move || name.get()
                                                on:input=move |ev| name.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div class="flex items-center gap-1 shrink-0">
                                            <PolishButton module_key="character" content=profile />
                                            <button
                                                class="text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-1 rounded hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors cursor-pointer"
                                                title="删除角色（移入回收站）"
                                                on:click=move |_| on_delete_char(del_id.clone())
                                            >
                                                "✕"
                                            </button>
                                        </div>
                                    </div>

                                    <textarea
                                        class="w-full text-xs text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-800 rounded p-1.5 focus:outline-hidden focus:ring-1 focus:ring-blue-500 min-h-[60px] leading-relaxed font-serif"
                                        placeholder="输入人物外貌、性格动机、身份背景与关系网……"
                                        prop:value=move || profile.get()
                                        on:input=move |ev| profile.set(event_target_value(&ev))
                                    ></textarea>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            } else {
                // Tab 2: Relations Graph View
                let all_chars = characters.get();
                let all_rels = relations.get();

                // Compute node positions for SVG
                let n = all_chars.len().max(1);
                let cx = 135.0;
                let cy = 115.0;
                let r = 80.0;

                let mut char_map = HashMap::new();
                let mut pos_map = HashMap::new();
                for (i, c) in all_chars.iter().enumerate() {
                    char_map.insert(c.id.clone(), c.name.clone());
                    let angle = (i as f64) * 2.0 * std::f64::consts::PI / (n as f64) - std::f64::consts::FRAC_PI_2;
                    let x = cx + r * angle.cos();
                    let y = cy + r * angle.sin();
                    pos_map.insert(c.id.clone(), (x, y));
                }

                view! {
                    <div class="space-y-4">
                        // 1. Add Relation Inline Form
                        <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-2.5 bg-slate-50/70 dark:bg-slate-850/70 space-y-2 text-xs">
                            <span class="font-bold text-slate-700 dark:text-slate-300 text-[11px]">"➕ 建立新关系连接"</span>
                            <div class="grid grid-cols-3 gap-1.5">
                                <select
                                    class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded px-1.5 py-1 text-[11px] text-slate-800 dark:text-slate-200"
                                    prop:value=move || new_from_id.get()
                                    on:change=move |ev| new_from_id.set(event_target_value(&ev))
                                >
                                    <option value="">"起点角色"</option>
                                    {all_chars.iter().map(|c| {
                                        view! { <option value=c.id.clone()>{c.name.clone()}</option> }
                                    }).collect::<Vec<_>>()}
                                </select>
                                <input
                                    type="text"
                                    class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded px-1.5 py-1 text-[11px] text-slate-800 dark:text-slate-200 placeholder-slate-400"
                                    placeholder="关系(如:师徒/宿敌)"
                                    prop:value=move || new_rel_name.get()
                                    on:input=move |ev| new_rel_name.set(event_target_value(&ev))
                                />
                                <select
                                    class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded px-1.5 py-1 text-[11px] text-slate-800 dark:text-slate-200"
                                    prop:value=move || new_to_id.get()
                                    on:change=move |ev| new_to_id.set(event_target_value(&ev))
                                >
                                    <option value="">"终点角色"</option>
                                    {all_chars.iter().map(|c| {
                                        view! { <option value=c.id.clone()>{c.name.clone()}</option> }
                                    }).collect::<Vec<_>>()}
                                </select>
                            </div>
                            <div class="flex items-center gap-1.5">
                                <input
                                    type="text"
                                    class="flex-1 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded px-1.5 py-1 text-[11px] text-slate-800 dark:text-slate-200 placeholder-slate-400"
                                    placeholder="关系补充说明(可选)"
                                    prop:value=move || new_desc.get()
                                    on:input=move |ev| new_desc.set(event_target_value(&ev))
                                />
                                <button
                                    class="bg-blue-600 hover:bg-blue-700 text-white px-2.5 py-1 rounded text-[11px] font-semibold transition-colors cursor-pointer shrink-0"
                                    on:click=on_create_relation
                                >
                                    "连接"
                                </button>
                            </div>
                        </div>

                        // 2. Interactive SVG Graph
                        <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-2 bg-white dark:bg-slate-900 shadow-2xs overflow-hidden flex flex-col items-center">
                            <span class="text-[10px] text-slate-400 self-start mb-1 font-medium">"可视化角色拓扑图："</span>
                            {if all_chars.is_empty() {
                                view! {
                                    <div class="py-12 text-center text-xs text-slate-400">
                                        "请先添加至少两位角色"
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <svg viewBox="0 0 270 230" class="w-full max-w-[270px] h-[230px]">
                                        // Relation edges
                                        {all_rels.iter().filter_map(|rel| {
                                            if let (Some(&(x1, y1)), Some(&(x2, y2))) = (pos_map.get(&rel.from_char_id), pos_map.get(&rel.to_char_id)) {
                                                let mid_x = (x1 + x2) / 2.0;
                                                let mid_y = (y1 + y2) / 2.0;
                                                Some(view! {
                                                    <g>
                                                        <line
                                                            x1=x1 y1=y1 x2=x2 y2=y2
                                                            stroke="#94a3b8"
                                                            stroke-width="1.5"
                                                            stroke-dasharray="3,2"
                                                            opacity="0.8"
                                                        />
                                                        <rect
                                                            x=mid_x - 18.0 y=mid_y - 7.0
                                                            width="36" height="14"
                                                            rx="3"
                                                            fill="#f8fafc"
                                                            stroke="#cbd5e1"
                                                            stroke-width="0.5"
                                                        />
                                                        <text
                                                            x=mid_x y=mid_y + 3.0
                                                            font-size="8"
                                                            text-anchor="middle"
                                                            fill="#2563eb"
                                                            font-weight="bold"
                                                        >
                                                            {rel.relation_name.clone()}
                                                        </text>
                                                    </g>
                                                })
                                            } else {
                                                None
                                            }
                                        }).collect::<Vec<_>>()}

                                        // Character nodes
                                        {all_chars.iter().map(|ch| {
                                            let (x, y) = pos_map.get(&ch.id).copied().unwrap_or((cx, cy));
                                            let display_name: String = ch.name.chars().take(2).collect();
                                            let full_name = ch.name.clone();

                                            view! {
                                                <g class="cursor-pointer group">
                                                    <circle
                                                        cx=x cy=y r="15"
                                                        fill="#3b82f6"
                                                        fill-opacity="0.15"
                                                        stroke="#3b82f6"
                                                        stroke-width="1.5"
                                                    />
                                                    <text
                                                        x=x y=y + 3.5
                                                        font-size="9"
                                                        text-anchor="middle"
                                                        font-weight="bold"
                                                        fill="#1e293b"
                                                        class="dark:fill-slate-100 select-none"
                                                    >
                                                        {display_name}
                                                    </text>
                                                    <text
                                                        x=x y=y + 24.0
                                                        font-size="8"
                                                        text-anchor="middle"
                                                        fill="#64748b"
                                                        class="dark:fill-slate-400 select-none"
                                                    >
                                                        {full_name}
                                                    </text>
                                                </g>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </svg>
                                }.into_any()
                            }}
                        </div>

                        // 3. Relations Detailed List
                        <div class="space-y-2">
                            <span class="text-xs font-bold text-slate-700 dark:text-slate-300">"已确认关系清单"</span>
                            {if all_rels.is_empty() {
                                view! {
                                    <div class="py-4 text-center text-xs text-slate-400">
                                        "暂无角色关系，请在上方表单中连接角色"
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-1.5">
                                        {all_rels.into_iter().map(|rel| {
                                            let r_id = rel.id.clone();
                                            let from_name = char_map.get(&rel.from_char_id).cloned().unwrap_or_else(|| "未知".to_string());
                                            let to_name = char_map.get(&rel.to_char_id).cloned().unwrap_or_else(|| "未知".to_string());
                                            let rel_name = rel.relation_name.clone();
                                            let desc = rel.description.clone();

                                            view! {
                                                <div class="border border-slate-200 dark:border-slate-800 rounded p-2 bg-white dark:bg-slate-900 shadow-2xs flex items-center justify-between gap-2 text-xs">
                                                    <div class="flex-1 min-w-0">
                                                        <div class="flex items-center gap-1.5 font-semibold text-slate-800 dark:text-slate-100">
                                                            <span class="text-blue-600 dark:text-blue-400">{from_name}</span>
                                                            <span class="px-1.5 py-0.2 rounded text-[10px] bg-blue-50 dark:bg-blue-950/80 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800">
                                                                {rel_name}
                                                            </span>
                                                            <span class="text-slate-400">"➔"</span>
                                                            <span class="text-blue-600 dark:text-blue-400">{to_name}</span>
                                                        </div>
                                                        {if !desc.is_empty() {
                                                            view! {
                                                                <p class="text-[11px] text-slate-500 dark:text-slate-400 truncate mt-0.5">
                                                                    {desc}
                                                                </p>
                                                            }.into_any()
                                                        } else {
                                                            view! { <div></div> }.into_any()
                                                        }}
                                                    </div>
                                                    <button
                                                        class="text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-1 rounded hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors cursor-pointer shrink-0"
                                                        title="解除此关系"
                                                        on:click=move |_| on_delete_relation(r_id.clone())
                                                    >
                                                        "✕"
                                                    </button>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

use crate::components::ai::polish_button::PolishButton;
use crate::components::settings_panel::character_explosion_modal::CharacterExplosionModal;
use crate::components::settings_panel::character_graph_view::CharacterGraphView;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{Character, CharacterRelation, ExplodedProfile};
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
    let active_sub_tab = RwSignal::new("cards".to_string()); // "cards" | "nebula" | "relations"
    let inspecting_character = RwSignal::new(Option::<Character>::None);

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
            // Sub-tab switcher: Cards vs Nebula Graph vs Relations Form
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
                                if active_sub_tab.get() == "nebula" {
                                    "bg-white dark:bg-slate-900 font-bold text-amber-500 dark:text-amber-400 shadow-2xs"
                                } else {
                                    "text-slate-500 dark:text-slate-400 hover:text-slate-700"
                                }
                            )
                        }
                        on:click=move |_| active_sub_tab.set("nebula".to_string())
                    >
                        "🌌 星云爆炸图"
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
                        "🔗 关系管理"
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
                } else if active_sub_tab.get() == "nebula" {
                    view! {
                        <span class="text-[10px] text-amber-600 dark:text-amber-400 font-medium">
                            "★ " {move || characters.get().len()} " 节点 / " {move || relations.get().len()} " 羁绊"
                        </span>
                    }.into_any()
                } else {
                    view! { <span class="text-[10px] text-slate-400">"共 " {move || relations.get().len()} " 条关系"</span> }.into_any()
                }}
            </div>

            // View 1: Cards View
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
                            let ch_modal = ch.clone();
                            let name = RwSignal::new(ch.name.clone());
                            let profile = RwSignal::new(ch.profile.clone());
                            let aliases = RwSignal::new(ch.aliases.clone());

                            let ep = ExplodedProfile::parse_from_str(&ch.profile);
                            let faction_name = if ep.faction.is_empty() { "默认阵营".to_string() } else { ep.faction };
                            let has_exploded = !ep.core.desire.is_empty() || !ep.appearance.features.is_empty();

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
                                            <span class="text-[10px] px-1.5 py-0.2 rounded bg-slate-100 dark:bg-slate-800 text-slate-500 shrink-0">
                                                {faction_name}
                                            </span>
                                        </div>
                                        <div class="flex items-center gap-1 shrink-0">
                                            // 6-Dimension Explosion Trigger Button
                                            <button
                                                class=format!(
                                                    "text-[11px] px-2 py-0.5 rounded font-semibold transition-colors cursor-pointer border {}",
                                                    if has_exploded {
                                                        "bg-purple-50 dark:bg-purple-950/70 text-purple-600 dark:text-purple-300 border-purple-200 dark:border-purple-800 hover:bg-purple-100"
                                                    } else {
                                                        "bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-400 border-slate-200 dark:border-slate-700 hover:bg-slate-200"
                                                    }
                                                )
                                                title="打开该角色的 6 维全息解构爆炸图"
                                                on:click=move |_| inspecting_character.set(Some(ch_modal.clone()))
                                            >
                                                "💥 6维爆炸"
                                            </button>
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

                                    {if has_exploded {
                                        view! {
                                            <div class="text-[11px] text-slate-600 dark:text-slate-300 bg-slate-50/80 dark:bg-slate-850/80 border border-slate-200 dark:border-slate-800 rounded p-2 space-y-1">
                                                {if !ep.core.desire.is_empty() {
                                                    view! { <p class="truncate"><span class="text-purple-600 dark:text-purple-400 font-semibold">"欲求："</span> {ep.core.desire}</p> }.into_any()
                                                } else { view! { <div></div> }.into_any() }}
                                                {if !ep.core.flaw.is_empty() {
                                                    view! { <p class="truncate"><span class="text-rose-600 dark:text-rose-400 font-semibold">"心魔："</span> {ep.core.flaw}</p> }.into_any()
                                                } else { view! { <div></div> }.into_any() }}
                                                {if !ep.appearance.features.is_empty() {
                                                    view! { <p class="truncate text-slate-500"><span class="text-emerald-600 dark:text-emerald-400 font-semibold">"特征："</span> {ep.appearance.features}</p> }.into_any()
                                                } else { view! { <div></div> }.into_any() }}
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <textarea
                                                class="w-full text-xs text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-800 rounded p-1.5 focus:outline-hidden focus:ring-1 focus:ring-blue-500 min-h-[55px] leading-relaxed font-serif"
                                                placeholder="输入人物外貌、性格动机、身份背景与关系网，或点击右上角「💥 6维爆炸」进行立体拆解……"
                                                prop:value=move || profile.get()
                                                on:input=move |ev| profile.set(event_target_value(&ev))
                                            ></textarea>
                                        }.into_any()
                                    }}
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            } else if active_sub_tab.get() == "nebula" {
                // View 2: High-Performance Canvas 2D Nebula Graph View
                view! {
                    <CharacterGraphView
                        characters=characters
                        relations=relations
                        on_inspect_character=Callback::new(move |ch| inspecting_character.set(Some(ch)))
                    />
                }.into_any()
            } else {
                // View 3: Relations List & Creation Form
                let all_chars = characters.get();
                let all_rels = relations.get();

                let mut char_map = HashMap::new();
                for c in all_chars.iter() {
                    char_map.insert(c.id.clone(), c.name.clone());
                }

                view! {
                    <div class="space-y-4">
                        // 1. Add Relation Form
                        <div class="border border-slate-200 dark:border-slate-800 rounded-lg p-2.5 bg-slate-50/70 dark:bg-slate-850/70 space-y-2 text-xs">
                            <span class="font-bold text-slate-700 dark:text-slate-300 text-[11px]">"➕ 建立新角色关系纽带"</span>
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
                                    "建立纽带"
                                </button>
                            </div>
                        </div>

                        // 2. Relations Detailed List
                        <div class="space-y-2">
                            <span class="text-xs font-bold text-slate-700 dark:text-slate-300">"已确认关系清单 (" {all_rels.len()} ")"</span>
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

            // 6-Dimension Exploded Modal Mount
            <CharacterExplosionModal
                app_ctx=app_ctx
                character=inspecting_character
                on_saved=Callback::new(move |_| {
                    load_characters();
                    load_relations();
                })
            />
        </div>
    }
}

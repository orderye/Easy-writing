use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::Character;

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

#[component]
pub fn CharactersView(app_ctx: AppContext) -> impl IntoView {
    let characters = RwSignal::new(Vec::<Character>::new());

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

    Effect::new(move |_| {
        load_characters();
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
        });
    };

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
            <div class="flex items-center justify-between text-xs pb-1 border-b border-slate-200 dark:border-slate-800">
                <span class="font-bold text-slate-700 dark:text-slate-300">"角色卡列表"</span>
                <button
                    class="text-xs text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-semibold px-2 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 transition-colors cursor-pointer"
                    on:click=on_add_char
                >
                    "+ 新增角色"
                </button>
            </div>

            {move || {
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
            }}
        </div>
    }
}

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

#[component]
pub fn CharactersView(app_ctx: AppContext) -> impl IntoView {
    let characters = RwSignal::new(Vec::<Character>::new());

    Effect::new(move |_| {
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

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto p-3 space-y-3">
            <div class="flex items-center justify-between">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"角色卡列表"</span>
                <button
                    class="text-xs text-blue-600 hover:text-blue-800 font-medium px-2 py-0.5 rounded hover:bg-blue-50 transition-colors"
                    on:click=on_add_char
                >
                    "+ 新增角色"
                </button>
            </div>

            <For
                each=move || characters.get()
                key=|c| c.id.clone()
                children=move |ch| {
                    let ch_id = ch.id.clone();
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
                        <div class="border border-slate-200 rounded-lg p-2.5 bg-white shadow-2xs space-y-2">
                            <div class="flex items-center justify-between gap-2">
                                <div class="flex items-center gap-1.5 flex-1 min-w-0">
                                    <span class="text-sm">"👤"</span>
                                    <input
                                        type="text"
                                        class="font-semibold text-xs text-slate-800 bg-transparent border-b border-transparent hover:border-slate-300 focus:border-blue-500 focus:outline-none flex-1 truncate"
                                        prop:value=move || name.get()
                                        on:input=move |ev| name.set(event_target_value(&ev))
                                    />
                                </div>
                                <PolishButton module_key="character" content=profile />
                            </div>

                            <textarea
                                class="w-full text-xs text-slate-600 bg-slate-50 border border-slate-200 rounded p-1.5 focus:bg-white focus:outline-blue-500 min-h-[60px]"
                                placeholder="输入人物外貌、性格动机、身份背景与关系网……"
                                prop:value=move || profile.get()
                                on:input=move |ev| profile.set(event_target_value(&ev))
                            ></textarea>
                        </div>
                    }
                }
            />
        </div>
    }
}

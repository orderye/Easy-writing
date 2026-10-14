use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::WorldviewEntry;

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

#[component]
pub fn WorldviewView(app_ctx: AppContext) -> impl IntoView {
    let entries = RwSignal::new(Vec::<WorldviewEntry>::new());

    // Load worldview entries when novel changes
    Effect::new(move |_| {
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
    });

    let on_add_entry = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<WorldviewEntry, _>(
                    "save_worldview",
                    &SaveWorldviewArgs {
                        id: None,
                        novel_id,
                        group_name: "通用".to_string(),
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

    view! {
        <div class="flex-1 overflow-y-auto p-3 space-y-3">
            <div class="flex items-center justify-between">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"世界观条目"</span>
                <button
                    class="text-xs text-blue-600 hover:text-blue-800 font-medium px-2 py-0.5 rounded hover:bg-blue-50 transition-colors"
                    on:click=on_add_entry
                >
                    "+ 新增条目"
                </button>
            </div>

            <For
                each=move || entries.get()
                key=|e| e.id.clone()
                children=move |entry| {
                    let e_id = entry.id.clone();
                    let group = entry.group_name.clone();
                    let title = RwSignal::new(entry.title.clone());
                    let content = RwSignal::new(entry.content.clone());

                    let save_entry = move |new_title: String, new_content: String| {
                        if let Some(novel) = app_ctx.current_novel.get() {
                            let nid = novel.id;
                            let eid = e_id.clone();
                            let grp = group.clone();
                            leptos::task::spawn_local(async move {
                                let _ = invoke::<WorldviewEntry, _>(
                                    "save_worldview",
                                    &SaveWorldviewArgs {
                                        id: Some(eid),
                                        novel_id: nid,
                                        group_name: grp,
                                        title: new_title,
                                        content: new_content,
                                    },
                                )
                                .await;
                            });
                        }
                    };

                    Effect::new(move |_| {
                        let t = title.get();
                        let c = content.get();
                        save_entry(t, c);
                    });

                    view! {
                        <div class="border border-slate-200 rounded-lg p-2.5 bg-white shadow-2xs space-y-1.5">
                            <div class="flex items-center justify-between gap-2">
                                <input
                                    type="text"
                                    class="font-semibold text-xs text-slate-800 bg-transparent border-b border-transparent hover:border-slate-300 focus:border-blue-500 focus:outline-none flex-1 truncate"
                                    prop:value=move || title.get()
                                    on:input=move |ev| title.set(event_target_value(&ev))
                                />
                                <PolishButton module_key="worldview" content=content />
                            </div>
                            <textarea
                                class="w-full text-xs text-slate-600 bg-slate-50 border border-slate-200 rounded p-1.5 focus:bg-white focus:outline-blue-500 min-h-[60px]"
                                placeholder="输入世界规则、地理、历史、派系术语……"
                                prop:value=move || content.get()
                                on:input=move |ev| content.set(event_target_value(&ev))
                            ></textarea>
                        </div>
                    }
                }
            />
        </div>
    }
}

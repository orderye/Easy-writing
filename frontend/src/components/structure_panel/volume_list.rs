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

#[allow(dead_code)]
#[derive(Serialize)]
struct UpdateVolumeArgs {
    id: String,
    title: String,
    summary: String,
}

#[component]
pub fn VolumeList(app_ctx: AppContext) -> impl IntoView {
    let volumes = move || app_ctx.volumes_list.get();

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

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-2">
            <div class="flex items-center justify-between px-1 mb-1">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"卷目与章节"</span>
                <button
                    class="text-xs text-blue-600 hover:text-blue-800 font-medium px-2 py-0.5 rounded hover:bg-blue-50 transition-colors"
                    on:click=on_create_volume
                >
                    "+ 新建卷"
                </button>
            </div>

            <For
                each=volumes
                key=|v| v.id.clone()
                children=move |vol| {
                    let vol_id = vol.id.clone();
                    let is_expanded = RwSignal::new(true);

                    view! {
                        <div class="border border-slate-200 rounded-lg bg-white overflow-hidden shadow-xs">
                            <div
                                class="px-3 py-2 bg-slate-50 hover:bg-slate-100 flex items-center justify-between cursor-pointer border-b border-slate-200 text-xs font-semibold text-slate-800"
                                on:click=move |_| is_expanded.update(|v| *v = !*v)
                            >
                                <div class="flex items-center gap-1.5 truncate">
                                    <span class="text-slate-400">
                                        {move || if is_expanded.get() { "▾" } else { "▸" }}
                                    </span>
                                    <span class="truncate">{vol.title.clone()}</span>
                                </div>
                            </div>

                            {move || {
                                if is_expanded.get() {
                                    view! {
                                        <ChapterList app_ctx=app_ctx volume_id=vol_id.clone() />
                                    }.into_any()
                                } else {
                                    view! { <div></div> }.into_any()
                                }
                            }}
                        </div>
                    }
                }
            />
        </div>
    }
}

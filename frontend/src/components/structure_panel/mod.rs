pub mod chapter_list;
pub mod save_button;
pub mod volume_list;

use crate::components::ai::polish_button::PolishButton;
use crate::components::layout::panel_toggle::RightToggle;
use crate::components::structure_panel::save_button::SaveButton;
use crate::components::structure_panel::volume_list::VolumeList;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct UpdateChapterMetaArgs {
    id: String,
    title: String,
    summary: String,
    status: String,
}

#[component]
pub fn StructurePanel(app_ctx: AppContext) -> impl IntoView {
    let current_chapter_summary = RwSignal::new(String::new());
    let current_chapter_status = RwSignal::new("draft".to_string());

    // Sync from current_chapter signal
    Effect::new(move |_| {
        if let Some(ch) = app_ctx.current_chapter.get() {
            current_chapter_summary.set(ch.summary);
            current_chapter_status.set(ch.status);
        }
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
                    let _ = invoke::<(), _>(
                        "update_chapter_meta",
                        &UpdateChapterMetaArgs {
                            id: cid,
                            title: ctitle,
                            summary: new_summary,
                            status: new_status,
                        },
                    )
                    .await;
                });
            }
        }
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
                    "h-full bg-slate-50 border-l border-slate-200 flex flex-col panel-transition select-none shrink-0 {}",
                    if app_ctx.right_panel_open.get() { "w-72" } else { "w-0 overflow-hidden border-none" }
                )
            }
        >
            <div class="h-12 border-b border-slate-200 px-3 flex items-center justify-between shrink-0">
                <span class="font-bold text-slate-800 text-sm">"作品结构"</span>
                <RightToggle is_open=app_ctx.right_panel_open />
            </div>

            <div class="p-3 border-b border-slate-200 shrink-0">
                <SaveButton app_ctx=app_ctx />
            </div>

            <VolumeList app_ctx=app_ctx />

            // Current chapter metadata & status card
            {move || {
                if let Some(ch) = app_ctx.current_chapter.get() {
                    view! {
                        <div class="border-t border-slate-200 p-3 bg-white space-y-2 shrink-0">
                            <div class="flex items-center justify-between text-xs">
                                <span class="font-semibold text-slate-700 truncate max-w-[120px]">
                                    {ch.title.clone()}
                                </span>
                                <select
                                    class="text-[11px] bg-slate-50 border border-slate-200 rounded px-1.5 py-0.5 focus:outline-blue-500 font-medium"
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
                                    <span class="text-[10px] text-slate-400 font-medium">"本章梗概摘要（同步章纲）"</span>
                                    <PolishButton module_key="chapter_summary" content=current_chapter_summary label="修饰" />
                                </div>
                                <textarea
                                    class="w-full text-xs text-slate-600 bg-slate-50 border border-slate-200 rounded p-1.5 focus:bg-white focus:outline-blue-500 min-h-[48px]"
                                    placeholder="输入单章核心摘要..."
                                    prop:value=move || current_chapter_summary.get()
                                    on:input=move |ev| current_chapter_summary.set(event_target_value(&ev))
                                ></textarea>
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

use crate::bridge::tiptap;
use crate::components::ai::polish_button::PolishButton;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::Chapter;

#[derive(Serialize)]
struct UpdateNovelArgs {
    id: String,
    title: String,
    synopsis: String,
}

#[derive(Serialize)]
struct UpdateVolumeArgs {
    id: String,
    title: String,
    summary: String,
}

#[derive(Serialize)]
struct UpdateChapterMetaArgs {
    id: String,
    title: String,
    summary: String,
    status: String,
}

#[derive(Serialize)]
struct GetChapterArgs {
    id: String,
}

#[component]
pub fn OutlineView(app_ctx: AppContext) -> impl IntoView {
    let synopsis = RwSignal::new(
        app_ctx
            .current_novel
            .get()
            .map(|n| n.synopsis)
            .unwrap_or_default(),
    );

    // Sync when novel changes
    Effect::new(move |_| {
        if let Some(n) = app_ctx.current_novel.get() {
            synopsis.set(n.synopsis);
        }
    });

    // Save novel synopsis on change
    Effect::new(move |_| {
        let val = synopsis.get();
        if let Some(mut novel) = app_ctx.current_novel.get() {
            if novel.synopsis != val {
                novel.synopsis = val.clone();
                app_ctx.current_novel.set(Some(novel.clone()));
                leptos::task::spawn_local(async move {
                    let _ = invoke::<(), _>(
                        "update_novel",
                        &UpdateNovelArgs {
                            id: novel.id,
                            title: novel.title,
                            synopsis: val,
                        },
                    )
                    .await;
                });
            }
        }
    });

    let volumes = move || app_ctx.volumes_list.get();

    let on_jump_to_chapter = move |chapter_id: String| {
        leptos::task::spawn_local(async move {
            if let Ok(Some(ch)) =
                invoke::<Option<Chapter>, _>("get_chapter", &GetChapterArgs { id: chapter_id }).await
            {
                tiptap::set_content(&ch.content);
                app_ctx.current_chapter.set(Some(ch));
                app_ctx.word_count.set(tiptap::get_word_count());
                app_ctx.save_status.set("saved".to_string());
            }
        });
    };

    view! {
        <div class="flex-1 overflow-y-auto p-3 space-y-4">
            // Layer 1: Whole novel synopsis
            <div>
                <div class="flex items-center justify-between mb-1.5">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"全篇梗概（总纲）"</span>
                    <PolishButton module_key="outline" content=synopsis />
                </div>
                <textarea
                    class="w-full text-xs text-slate-700 bg-white border border-slate-200 rounded p-2 focus:outline-blue-500 min-h-[70px]"
                    placeholder="输入小说的核心主线、主要悬念与高潮走向……"
                    prop:value=move || synopsis.get()
                    on:input=move |ev| synopsis.set(event_target_value(&ev))
                ></textarea>
            </div>

            // Layer 2 & 3: Volumes (卷纲) & Chapters (章纲)
            <div class="space-y-3">
                <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider block">"卷纲与章纲列表"</span>
                <For
                    each=volumes
                    key=|v| v.id.clone()
                    children=move |vol| {
                        let vol_id = vol.id.clone();
                        let vol_title = vol.title.clone();
                        let summary = RwSignal::new(vol.summary.clone());

                        let vol_id_for_chs = vol_id.clone();
                        let chs = move || {
                            let vid = vol_id_for_chs.clone();
                            app_ctx
                                .chapters_map
                                .get()
                                .into_iter()
                                .filter(|c| c.volume_id == vid)
                                .collect::<Vec<_>>()
                        };

                        let v_id_for_save = vol_id.clone();
                        let v_title_for_save = vol_title.clone();
                        Effect::new(move |_| {
                            let new_val = summary.get();
                            let vid = v_id_for_save.clone();
                            let vtitle = v_title_for_save.clone();
                            leptos::task::spawn_local(async move {
                                let _ = invoke::<(), _>(
                                    "update_volume",
                                    &UpdateVolumeArgs {
                                        id: vid,
                                        title: vtitle,
                                        summary: new_val,
                                    },
                                )
                                .await;
                            });
                        });

                        view! {
                            <div class="border border-slate-200 rounded-lg p-2.5 bg-white shadow-2xs space-y-2">
                                <div class="flex items-center justify-between">
                                    <span class="font-bold text-xs text-slate-800">{vol_title.clone()}</span>
                                    <PolishButton module_key="volume_summary" content=summary label="修饰卷纲" />
                                </div>
                                <textarea
                                    class="w-full text-xs text-slate-600 bg-slate-50 border border-slate-200 rounded p-1.5 focus:bg-white focus:outline-blue-500 min-h-[45px]"
                                    placeholder="提炼本卷主题与走向（卷纲）..."
                                    prop:value=move || summary.get()
                                    on:input=move |ev| summary.set(event_target_value(&ev))
                                ></textarea>

                                // Chapters under this volume
                                <div class="pl-2 border-l-2 border-slate-100 space-y-1.5 pt-1">
                                    <For
                                        each=chs
                                        key=|c| c.id.clone()
                                        children=move |ch| {
                                            let ch_id = ch.id.clone();
                                            let ch_id_for_jump = ch_id.clone();
                                            let ch_title = ch.title.clone();
                                            let ch_summary = RwSignal::new(ch.summary.clone());

                                            let c_id_for_save = ch_id.clone();
                                            let c_title_for_save = ch.title.clone();
                                            let c_status_for_save = ch.status.clone();
                                            Effect::new(move |_| {
                                                let new_s = ch_summary.get();
                                                let cid = c_id_for_save.clone();
                                                let ctitle = c_title_for_save.clone();
                                                let cstat = c_status_for_save.clone();
                                                leptos::task::spawn_local(async move {
                                                    let _ = invoke::<(), _>(
                                                        "update_chapter_meta",
                                                        &UpdateChapterMetaArgs {
                                                            id: cid,
                                                            title: ctitle,
                                                            summary: new_s,
                                                            status: cstat,
                                                        },
                                                    )
                                                    .await;
                                                });
                                            });

                                            view! {
                                                <div class="text-[11px] bg-slate-50/70 p-1.5 rounded border border-slate-100 space-y-1">
                                                    <div class="flex items-center justify-between">
                                                        <span
                                                            class="font-semibold text-blue-600 hover:text-blue-800 cursor-pointer truncate"
                                                            title="点击跳转并编辑此章节"
                                                            on:click=move |_| on_jump_to_chapter(ch_id_for_jump.clone())
                                                        >
                                                            "📄 " {ch_title}
                                                        </span>
                                                        <div class="flex items-center gap-1.5">
                                                            <span class="text-[9px] text-slate-400">
                                                                {ch.word_count} "字"
                                                            </span>
                                                            <PolishButton module_key="chapter_summary" content=ch_summary label="修饰" />
                                                        </div>
                                                    </div>
                                                    <input
                                                        type="text"
                                                        class="w-full text-[10px] text-slate-500 bg-white border border-slate-200 rounded px-1.5 py-0.5 focus:outline-blue-500"
                                                        placeholder="章纲摘要（一两句话总结）..."
                                                        prop:value=move || ch_summary.get()
                                                        on:input=move |ev| ch_summary.set(event_target_value(&ev))
                                                    />
                                                </div>
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}

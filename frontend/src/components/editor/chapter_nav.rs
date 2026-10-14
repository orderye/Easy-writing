use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::Chapter;

#[derive(Serialize)]
struct GetChapterArgs {
    id: String,
}

#[component]
pub fn ChapterNav(app_ctx: AppContext) -> impl IntoView {
    let volume_title = move || {
        app_ctx
            .current_volume
            .get()
            .map(|v| v.title)
            .unwrap_or_else(|| "默认卷".to_string())
    };

    let chapter_title = move || {
        app_ctx
            .current_chapter
            .get()
            .map(|c| c.title)
            .unwrap_or_else(|| "请选择或新建章节".to_string())
    };

    let novel_title = move || {
        app_ctx
            .current_novel
            .get()
            .map(|n| n.title)
            .unwrap_or_else(|| "Easy-writing".to_string())
    };

    // Quick switch to previous chapter
    let on_prev_chapter = move |_| {
        let headers = app_ctx.chapters_map.get();
        if let Some(curr) = app_ctx.current_chapter.get() {
            if let Some(idx) = headers.iter().position(|h| h.id == curr.id) {
                if idx > 0 {
                    let target_id = headers[idx - 1].id.clone();
                    leptos::task::spawn_local(async move {
                        if let Ok(Some(ch)) =
                            invoke::<Option<Chapter>, _>("get_chapter", &GetChapterArgs { id: target_id }).await
                        {
                            tiptap::set_content(&ch.content);
                            app_ctx.current_chapter.set(Some(ch));
                            app_ctx.word_count.set(tiptap::get_word_count());
                            app_ctx.save_status.set("saved".to_string());
                        }
                    });
                }
            }
        }
    };

    // Quick switch to next chapter
    let on_next_chapter = move |_| {
        let headers = app_ctx.chapters_map.get();
        if let Some(curr) = app_ctx.current_chapter.get() {
            if let Some(idx) = headers.iter().position(|h| h.id == curr.id) {
                if idx + 1 < headers.len() {
                    let target_id = headers[idx + 1].id.clone();
                    leptos::task::spawn_local(async move {
                        if let Ok(Some(ch)) =
                            invoke::<Option<Chapter>, _>("get_chapter", &GetChapterArgs { id: target_id }).await
                        {
                            tiptap::set_content(&ch.content);
                            app_ctx.current_chapter.set(Some(ch));
                            app_ctx.word_count.set(tiptap::get_word_count());
                            app_ctx.save_status.set("saved".to_string());
                        }
                    });
                }
            }
        }
    };

    view! {
        <div class="h-12 border-b border-slate-200 bg-white px-4 flex items-center justify-between shadow-sm select-none z-10">
            <div class="flex items-center gap-3">
                // Novel title shown when left panel is hidden
                {move || {
                    if !app_ctx.left_panel_open.get() {
                        view! {
                            <span class="font-bold text-slate-800 text-sm border-r border-slate-300 pr-3 mr-1">
                                {novel_title()}
                            </span>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}

                <div class="flex items-center text-sm text-slate-600 gap-1.5 font-medium">
                    <span class="text-slate-400">"当前位置:"</span>
                    <span class="hover:text-blue-600 cursor-pointer">{volume_title}</span>
                    <span class="text-slate-300">"›"</span>
                    <span class="text-slate-900 font-semibold">{chapter_title}</span>
                </div>

                <div class="flex items-center gap-1 ml-2">
                    <button
                        class="p-1 rounded hover:bg-slate-100 text-slate-500 hover:text-slate-800 text-xs"
                        title="上一章"
                        on:click=on_prev_chapter
                    >
                        "▲ 上一章"
                    </button>
                    <button
                        class="p-1 rounded hover:bg-slate-100 text-slate-500 hover:text-slate-800 text-xs"
                        title="下一章"
                        on:click=on_next_chapter
                    >
                        "▼ 下一章"
                    </button>
                </div>
            </div>

            <div class="flex items-center gap-3">
                <button
                    class="px-3 py-1 bg-amber-50 hover:bg-amber-100 text-amber-700 border border-amber-300 rounded text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-sm cursor-pointer"
                    title="从当前正文提炼世界观、角色、剧情和大纲"
                    on:click=move |_| {
                        if let (Some(novel), Some(chapter)) = (app_ctx.current_novel.get(), app_ctx.current_chapter.get()) {
                            let nid = novel.id;
                            let cid = chapter.id;
                            app_ctx.is_extract_modal_open.set(true);
                            app_ctx.is_extracting.set(true);
                            leptos::task::spawn_local(async move {
                                #[derive(Serialize)]
                                struct TriggerExtractArgs {
                                    novel_id: String,
                                    chapter_id: String,
                                }
                                if let Ok(candidates) = invoke::<Vec<shared_types::ExtractionCandidate>, _>(
                                    "trigger_chapter_extract",
                                    &TriggerExtractArgs {
                                        novel_id: nid,
                                        chapter_id: cid,
                                    },
                                )
                                .await {
                                    app_ctx.extraction_candidates.set(candidates);
                                }
                                app_ctx.is_extracting.set(false);
                            });
                        }
                    }
                >
                    <span class="text-amber-500">"✨"</span>
                    "提炼本章"
                </button>

                // Save button shown in nav bar when right panel is hidden
                {move || {
                    if !app_ctx.right_panel_open.get() {
                        let save_status = app_ctx.save_status.get();
                        let is_dirty = save_status == "dirty";
                        view! {
                            <button
                                class=format!(
                                    "px-3 py-1 text-xs font-medium rounded border transition-colors cursor-pointer {}",
                                    if is_dirty {
                                        "bg-blue-600 text-white border-blue-700 hover:bg-blue-700"
                                    } else {
                                        "bg-slate-100 text-slate-600 border-slate-300 hover:bg-slate-200"
                                    }
                                )
                                on:click=move |_| {
                                    crate::components::structure_panel::save_button::perform_save(app_ctx);
                                }
                            >
                                {if is_dirty { "保存修改 *" } else { "已保存" }}
                            </button>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

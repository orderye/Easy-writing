use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;

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
                    crate::components::structure_panel::chapter_list::request_switch_chapter(
                        app_ctx,
                        headers[idx - 1].clone(),
                    );
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
                    crate::components::structure_panel::chapter_list::request_switch_chapter(
                        app_ctx,
                        headers[idx + 1].clone(),
                    );
                }
            }
        }
    };

    let refresh_candidates = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let nid = novel.id;
            leptos::task::spawn_local(async move {
                #[derive(Serialize)]
                struct ListCandidatesArgs {
                    novel_id: String,
                }
                if let Ok(cands) = invoke::<Vec<shared_types::ExtractionCandidate>, _>(
                    "list_extraction_candidates",
                    &ListCandidatesArgs { novel_id: nid },
                )
                .await
                {
                    app_ctx.extraction_candidates.set(cands);
                }
            });
        }
    };

    Effect::new(move |_| {
        let _ = app_ctx.current_chapter.get();
        refresh_candidates();
    });

    let cand_count = move || app_ctx.extraction_candidates.get().len();

    let on_click_extract = move |_| {
        let count = cand_count();
        if count > 0 {
            app_ctx.is_extract_modal_open.set(true);
        } else if let (Some(novel), Some(chapter)) = (app_ctx.current_novel.get(), app_ctx.current_chapter.get()) {
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
    };

    view! {
        <div class="h-12 border-b border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 flex items-center justify-between shadow-xs select-none z-10 shrink-0">
            <div class="flex items-center gap-2">
                // Left Panel unfold button & novel title shown when left panel is hidden
                {move || {
                    if !app_ctx.left_panel_open.get() {
                        view! {
                            <div class="flex items-center gap-2 mr-2">
                                <button
                                    class="px-2 py-1 text-xs font-semibold rounded bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-200 flex items-center gap-1 transition-colors border border-slate-300 dark:border-slate-600 cursor-pointer"
                                    title="展开设定库"
                                    on:click=move |_| app_ctx.left_panel_open.set(true)
                                >
                                    "▶ 设定"
                                </button>
                                <span class="font-bold text-slate-800 dark:text-slate-100 text-sm border-r border-slate-300 dark:border-slate-600 pr-3">
                                    {novel_title()}
                                </span>
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}

                <div class="flex items-center text-sm text-slate-600 dark:text-slate-300 gap-1.5 font-medium">
                    <span class="text-slate-400 dark:text-slate-500">"当前位置:"</span>
                    <span class="hover:text-blue-600 dark:hover:text-blue-400 cursor-pointer">{volume_title}</span>
                    <span class="text-slate-300 dark:text-slate-600">"›"</span>
                    <span class="text-slate-900 dark:text-slate-100 font-semibold">{chapter_title}</span>
                </div>

                <div class="flex items-center gap-1 ml-2">
                    <button
                        class="p-1 rounded hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 text-xs cursor-pointer"
                        title="上一章"
                        on:click=on_prev_chapter
                    >
                        "▲ 上一章"
                    </button>
                    <button
                        class="p-1 rounded hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 text-xs cursor-pointer"
                        title="下一章"
                        on:click=on_next_chapter
                    >
                        "▼ 下一章"
                    </button>
                </div>
            </div>

            <div class="flex items-center gap-2">
                <button
                    class="px-2.5 py-1 bg-slate-100 hover:bg-slate-200 dark:bg-slate-700 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-200 border border-slate-300 dark:border-slate-600 rounded text-xs font-medium flex items-center gap-1.5 transition-colors shadow-xs cursor-pointer"
                    title="查看章节版本历史与时间轴回滚"
                    on:click=move |_| {
                        if app_ctx.current_chapter.get().is_some() {
                            app_ctx.is_revisions_modal_open.set(true);
                        }
                    }
                >
                    <span class="text-slate-500 dark:text-slate-400">"🕒"</span>
                    "历史版本"
                </button>

                <button
                    class="px-3 py-1 bg-amber-50 hover:bg-amber-100 dark:bg-amber-950/40 dark:hover:bg-amber-900/60 text-amber-700 dark:text-amber-300 border border-amber-300 dark:border-amber-700/60 rounded text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-xs cursor-pointer"
                    title="从当前正文提炼世界观、角色、剧情和大纲"
                    on:click=on_click_extract
                >
                    <span class="text-amber-500">"✨"</span>
                    <span>"提炼本章"</span>
                    {move || {
                        let count = cand_count();
                        if count > 0 {
                            view! {
                                <span class="px-1.5 py-0.2 text-[10px] bg-rose-500 text-white rounded-full font-bold shadow-xs">
                                    {count}
                                </span>
                            }.into_any()
                        } else {
                            view! { <span class="hidden"></span> }.into_any()
                        }
                    }}
                </button>

                // Save button & right panel unfold shown when right panel is hidden
                {move || {
                    if !app_ctx.right_panel_open.get() {
                        let save_status = app_ctx.save_status.get();
                        let is_dirty = save_status == "dirty";
                        view! {
                            <div class="flex items-center gap-2">
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
                                <button
                                    class="px-2 py-1 text-xs font-semibold rounded bg-slate-100 hover:bg-slate-200 text-slate-700 flex items-center gap-1 transition-colors border border-slate-300 cursor-pointer"
                                    title="展开结构目录"
                                    on:click=move |_| app_ctx.right_panel_open.set(true)
                                >
                                    "目录 ◀"
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

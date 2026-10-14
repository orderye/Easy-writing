use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{Chapter, ChapterHeader};

#[derive(Serialize)]
struct GetChapterArgs {
    id: String,
}

#[derive(Serialize)]
struct CreateChapterArgs {
    novel_id: String,
    volume_id: String,
    title: String,
    summary: String,
}

#[derive(Serialize)]
struct DeleteChapterArgs {
    id: String,
}

pub fn do_switch_chapter(app_ctx: AppContext, chapter_id: String) {
    if chapter_id.is_empty() {
        return;
    }
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
}

pub fn request_switch_chapter(app_ctx: AppContext, target: ChapterHeader) {
    if let Some(curr) = app_ctx.current_chapter.get() {
        if curr.id == target.id {
            return;
        }
    }
    if app_ctx.save_status.get() == "dirty" {
        app_ctx.pending_switch_chapter.set(Some(target));
        app_ctx.is_unsaved_modal_open.set(true);
    } else {
        do_switch_chapter(app_ctx, target.id);
    }
}

pub fn create_chapter_in_volume(app_ctx: AppContext, volume_id: String) {
    if let Some(novel) = app_ctx.current_novel.get() {
        let novel_id = novel.id;
        let v_id = volume_id.clone();
        leptos::task::spawn_local(async move {
            let res = invoke::<Chapter, _>(
                "create_chapter",
                &CreateChapterArgs {
                    novel_id,
                    volume_id: v_id,
                    title: "新章节".to_string(),
                    summary: "".to_string(),
                },
            )
            .await;

            if let Ok(new_ch) = res {
                let mut list = app_ctx.chapters_map.get();
                let new_header = ChapterHeader {
                    id: new_ch.id.clone(),
                    novel_id: new_ch.novel_id.clone(),
                    volume_id: new_ch.volume_id.clone(),
                    title: new_ch.title.clone(),
                    summary: new_ch.summary.clone(),
                    word_count: 0,
                    status: new_ch.status.clone(),
                    sort_order: new_ch.sort_order,
                    updated_at: new_ch.updated_at.clone(),
                };
                list.push(new_header.clone());
                app_ctx.chapters_map.set(list);

                // Switch to new chapter (checking if current chapter is dirty)
                request_switch_chapter(app_ctx, new_header);
            }
        });
    }
}

pub fn delete_chapter_by_id(app_ctx: AppContext, ch_id: String) {
    leptos::task::spawn_local(async move {
        let _ = invoke::<(), _>("delete_chapter", &DeleteChapterArgs { id: ch_id.clone() }).await;
        let mut list = app_ctx.chapters_map.get();
        list.retain(|c| c.id != ch_id);
        app_ctx.chapters_map.set(list.clone());

        if let Some(curr) = app_ctx.current_chapter.get() {
            if curr.id == ch_id {
                if let Some(next_ch) = list.first() {
                    do_switch_chapter(app_ctx, next_ch.id.clone());
                } else {
                    app_ctx.current_chapter.set(None);
                }
            }
        }
    });
}

#[allow(dead_code)]
#[component]
pub fn ChapterList(app_ctx: AppContext, volume_id: String) -> impl IntoView {
    let vol_id = volume_id.clone();
    let vol_id_for_create = volume_id.clone();

    let chapters_for_vol = move || {
        let q = app_ctx.chapter_search_query.get().to_lowercase().trim().to_string();
        let st = app_ctx.chapter_status_filter.get();
        app_ctx
            .chapters_map
            .get()
            .into_iter()
            .filter(|c| {
                if c.volume_id != vol_id {
                    return false;
                }
                if !q.is_empty() && !c.title.to_lowercase().contains(&q) {
                    return false;
                }
                if st != "all" && c.status != st {
                    return false;
                }
                true
            })
            .collect::<Vec<ChapterHeader>>()
    };

    view! {
        <div class="pl-4 pr-1 py-1 space-y-1">
            <For
                each=chapters_for_vol
                key=|c| c.id.clone()
                children=move |ch| {
                    let ch_id = ch.id.clone();
                    let del_cid = ch.id.clone();
                    let ch_clone = ch.clone();
                    let ch_status = ch.status.clone();
                    let is_active = move || {
                        app_ctx
                            .current_chapter
                            .get()
                            .map(|c| c.id == ch_id)
                            .unwrap_or(false)
                    };

                    view! {
                        <div
                            class=move || {
                                format!(
                                    "group flex items-center justify-between px-2.5 py-1.5 rounded text-xs cursor-pointer transition-colors {}",
                                    if is_active() {
                                        "bg-blue-50 text-blue-700 font-semibold border-l-2 border-blue-600 dark:bg-blue-950/60 dark:text-blue-300 dark:border-blue-400"
                                    } else {
                                        "text-slate-700 hover:bg-slate-100 hover:text-slate-900 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-slate-100"
                                    }
                                )
                            }
                            on:click=move |_| request_switch_chapter(app_ctx, ch_clone.clone())
                        >
                            <div class="flex items-center gap-1.5 truncate flex-1 min-w-0 pr-1">
                                <span class="text-slate-400 group-hover:text-slate-600 dark:group-hover:text-slate-300 text-[11px] shrink-0">"📄"</span>
                                <span class="truncate">{ch.title.clone()}</span>
                            </div>

                            <div class="flex items-center gap-1.5 shrink-0 text-[10px]">
                                // Status Pill Badge
                                {match ch_status.as_str() {
                                    "finalized" => view! {
                                        <span class="px-1 py-0.2 rounded font-medium bg-emerald-100 dark:bg-emerald-950/80 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
                                            "定稿"
                                        </span>
                                    }.into_any(),
                                    "editing" => view! {
                                        <span class="px-1 py-0.2 rounded font-medium bg-blue-100 dark:bg-blue-950/80 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800">
                                            "修改"
                                        </span>
                                    }.into_any(),
                                    _ => view! {
                                        <span class="px-1 py-0.2 rounded font-medium bg-amber-100 dark:bg-amber-950/80 text-amber-700 dark:text-amber-300 border border-amber-200 dark:border-amber-800">
                                            "草稿"
                                        </span>
                                    }.into_any(),
                                }}
                                <span class="text-slate-400 dark:text-slate-500 font-normal">{ch.word_count} "字"</span>
                                <button
                                    class="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-0.5 rounded transition-opacity cursor-pointer shrink-0"
                                    title="删除章节（移入回收站）"
                                    on:click=move |ev| {
                                        ev.stop_propagation();
                                        delete_chapter_by_id(app_ctx, del_cid.clone());
                                    }
                                >
                                    "✕"
                                </button>
                            </div>
                        </div>
                    }
                }
            />

            <button
                class="w-full text-left px-2.5 py-1 text-[11px] text-slate-400 hover:text-blue-600 hover:bg-slate-100 dark:hover:bg-slate-800 dark:hover:text-blue-400 rounded flex items-center gap-1 transition-colors cursor-pointer"
                on:click=move |_| create_chapter_in_volume(app_ctx, vol_id_for_create.clone())
            >
                <span>"+"</span>
                <span>"添加新章节"</span>
            </button>
        </div>
    }
}

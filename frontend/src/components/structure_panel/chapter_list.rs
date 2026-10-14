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

#[allow(dead_code)]
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

#[component]
pub fn ChapterList(app_ctx: AppContext, volume_id: String) -> impl IntoView {
    let vol_id = volume_id.clone();
    let vol_id_for_create = volume_id.clone();

    let chapters_for_vol = move || {
        app_ctx
            .chapters_map
            .get()
            .into_iter()
            .filter(|c| c.volume_id == vol_id)
            .collect::<Vec<ChapterHeader>>()
    };

    let on_create_chapter = move |_| {
        let v_id = vol_id_for_create.clone();
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
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
    };

    view! {
        <div class="pl-4 pr-1 py-1 space-y-1">
            <For
                each=chapters_for_vol
                key=|c| c.id.clone()
                children=move |ch| {
                    let ch_id = ch.id.clone();
                    let ch_clone = ch.clone();
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
                            <div class="flex items-center gap-1.5 truncate">
                                <span class="text-slate-400 group-hover:text-slate-600 dark:group-hover:text-slate-300">"📄"</span>
                                <span class="truncate">{ch.title.clone()}</span>
                            </div>

                            <div class="flex items-center gap-1.5 text-[10px] text-slate-400 dark:text-slate-500">
                                <span>{ch.word_count} "字"</span>
                            </div>
                        </div>
                    }
                }
            />

            <button
                class="w-full text-left px-2.5 py-1 text-[11px] text-slate-400 hover:text-blue-600 hover:bg-slate-100 dark:hover:bg-slate-800 dark:hover:text-blue-400 rounded flex items-center gap-1 transition-colors cursor-pointer"
                on:click=on_create_chapter
            >
                <span>"+"</span>
                <span>"添加新章节"</span>
            </button>
        </div>
    }
}

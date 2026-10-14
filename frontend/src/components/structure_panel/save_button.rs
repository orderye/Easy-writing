use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct SaveChapterArgs {
    id: String,
    content: String,
    content_plain: String,
    word_count: i32,
}

pub fn perform_save(app_ctx: AppContext) {
    if let Some(mut chapter) = app_ctx.current_chapter.get() {
        app_ctx.save_status.set("saving".to_string());
        let content_json = tiptap::get_content_json();
        let content_plain = tiptap::get_content_plain();
        let word_count = tiptap::get_word_count() as i32;

        chapter.content = content_json.clone();
        chapter.content_plain = content_plain.clone();
        chapter.word_count = word_count;
        app_ctx.current_chapter.set(Some(chapter));
        app_ctx.word_count.set(word_count as u32);

        let chapter_id = app_ctx.current_chapter.get().unwrap().id;

        leptos::task::spawn_local(async move {
            let res = invoke::<(), _>(
                "save_chapter_content",
                &SaveChapterArgs {
                    id: chapter_id,
                    content: content_json,
                    content_plain,
                    word_count,
                },
            )
            .await;

            match res {
                Ok(_) => {
                    app_ctx.save_status.set("saved".to_string());
                }
                Err(err) => {
                    web_sys::console::error_1(&format!("Failed to save chapter: {}", err).into());
                    app_ctx.save_status.set("error".to_string());
                }
            }
        });
    }
}

#[component]
pub fn SaveButton(app_ctx: AppContext) -> impl IntoView {
    let save_status = move || app_ctx.save_status.get();
    let is_dirty = move || save_status() == "dirty";

    view! {
        <button
            class=move || {
                format!(
                    "w-full py-2 px-3 rounded-md text-sm font-semibold flex items-center justify-center gap-2 shadow-sm transition-all {}",
                    if is_dirty() {
                        "bg-blue-600 hover:bg-blue-700 text-white animate-pulse"
                    } else if save_status() == "saving" {
                        "bg-slate-200 text-slate-500 cursor-wait"
                    } else if save_status() == "error" {
                        "bg-red-500 text-white"
                    } else {
                        "bg-slate-100 hover:bg-slate-200 text-slate-700 border border-slate-300"
                    }
                )
            }
            on:click=move |_| perform_save(app_ctx)
        >
            <span>
                {move || match save_status().as_str() {
                    "dirty" => "💾 保存当前章节 *",
                    "saving" => "⏳ 正在保存...",
                    "error" => "⚠️ 保存失败 (点击重试)",
                    _ => "✓ 章节已保存",
                }}
            </span>
            <span class="text-xs opacity-75 font-normal">"(⌘S)"</span>
        </button>
    }
}

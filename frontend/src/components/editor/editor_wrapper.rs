use crate::bridge::tiptap;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::LexiconEntry;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

#[component]
pub fn EditorWrapper(app_ctx: AppContext) -> impl IntoView {
    // Mount editor on client side once DOM element is present
    Effect::new(move |_| {
        if !app_ctx.is_editor_mounted.get() {
            let initial_json = app_ctx
                .current_chapter
                .get()
                .map(|c| c.content)
                .unwrap_or_else(|| "{}".to_string());

            let on_update = {
                let save_status = app_ctx.save_status;
                let word_count = app_ctx.word_count;
                Closure::wrap(Box::new(move || {
                    save_status.set("dirty".to_string());
                    word_count.set(tiptap::get_word_count());
                }) as Box<dyn FnMut()>)
            };

            let mounted = tiptap::mount_editor(
                "tiptap-editor-root",
                &initial_json,
                on_update.as_ref().unchecked_ref(),
            );

            if mounted {
                on_update.forget(); // Keep the closure alive for the lifecycle of the editor

                // Register ghost completion acceptance callback
                let on_term_accepted = {
                    let current_novel = app_ctx.current_novel;
                    Closure::wrap(Box::new(move |term: String| {
                        if let Some(novel) = current_novel.get_untracked() {
                            let novel_id = novel.id;
                            leptos::task::spawn_local(async move {
                                let _ = invoke_cmd("record_lexicon_usage")
                                    .arg("novel_id", novel_id)
                                    .arg("term", term)
                                    .send_unit()
                                    .await;
                            });
                        }
                    }) as Box<dyn FnMut(String)>)
                };
                tiptap::on_lexicon_term_accepted(on_term_accepted.as_ref().unchecked_ref());
                on_term_accepted.forget();

                app_ctx.is_editor_mounted.set(true);
                app_ctx.word_count.set(tiptap::get_word_count());
            }
        }
    });

    // Effect: Load lexicon whenever current novel or lexicon_version changes
    Effect::new(move |_| {
        let _version = app_ctx.lexicon_version.get();
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let list_res = invoke_cmd("list_lexicon")
                    .arg("novel_id", novel_id.clone())
                    .send::<Vec<LexiconEntry>>()
                    .await;

                match list_res {
                    Ok(list) => {
                        if list.is_empty() {
                            // Automatically initialize lexicon from settings & chapters
                            let _ = invoke_cmd("sync_lexicon_from_novel")
                                .arg("novel_id", novel_id.clone())
                                .send::<usize>()
                                .await;
                            // Fetch again
                            if let Ok(refreshed) = invoke_cmd("list_lexicon")
                                .arg("novel_id", novel_id)
                                .send::<Vec<LexiconEntry>>()
                                .await
                            {
                                if let Ok(json_str) = serde_json::to_string(&refreshed) {
                                    tiptap::set_lexicon(&json_str);
                                }
                            }
                        } else if let Ok(json_str) = serde_json::to_string(&list) {
                            tiptap::set_lexicon(&json_str);
                        }
                    }
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("[EditorWrapper] Failed to fetch lexicon: {:?}", e).into(),
                        );
                    }
                }
            });
        }
    });

    // Effect: Sync ghost completion enabled flag with bridge
    Effect::new(move |_| {
        let enabled = app_ctx.is_ghost_completion_enabled.get();
        tiptap::set_ghost_completion_enabled(enabled);
    });

    view! {
        <div class="flex-1 min-h-0 overflow-y-auto px-8 md:px-16 py-8 bg-white dark:bg-slate-900 cursor-text flex justify-center">
            <div class="max-w-3xl w-full min-h-full pb-32">
                <div id="tiptap-editor-root" class="min-h-full"></div>
            </div>
        </div>
    }
}

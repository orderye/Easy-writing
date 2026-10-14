use crate::bridge::tiptap;
use crate::state::AppContext;
use leptos::prelude::*;
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
                app_ctx.is_editor_mounted.set(true);
                app_ctx.word_count.set(tiptap::get_word_count());
            }
        }
    });

    view! {
        <div class="flex-1 overflow-y-auto px-12 py-8 bg-white cursor-text flex justify-center">
            <div class="max-w-3xl w-full min-h-[500px]">
                <div id="tiptap-editor-root" class="min-h-[500px]"></div>
            </div>
        </div>
    }
}

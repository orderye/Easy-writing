use crate::components::ai::conflict_modal::ConflictModal;
use crate::components::ai::extraction_modal::ExtractionModal;
use crate::components::config::config_modal::ConfigModal;
use crate::components::layout::writing_stage::WritingStage;
use crate::components::settings_panel::SettingsPanel;
use crate::components::structure_panel::save_button::perform_save;
use crate::components::structure_panel::StructurePanel;
use crate::state::AppContext;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

#[component]
pub fn AppShell(app_ctx: AppContext) -> impl IntoView {
    // Register global keyboard shortcut (Cmd+S / Ctrl+S)
    Effect::new(move |_| {
        let ctx = app_ctx;
        let window = web_sys::window().expect("global window does not exist");
        let document = window.document().expect("document does not exist");

        let key_listener = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if (event.meta_key() || event.ctrl_key()) && event.key() == "s" {
                event.prevent_default();
                perform_save(ctx);
            }
        }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

        document
            .add_event_listener_with_callback("keydown", key_listener.as_ref().unchecked_ref())
            .expect("failed to add keydown listener");

        key_listener.forget();
    });

    // Background auto-save loop: every 30 seconds, if dirty, perform save
    Effect::new(move |_| {
        let ctx = app_ctx;
        leptos::task::spawn_local(async move {
            loop {
                TimeoutFuture::new(30_000).await;
                if ctx.save_status.get() == "dirty" {
                    perform_save(ctx);
                }
            }
        });
    });

    view! {
        <div class="h-full w-full flex flex-row overflow-hidden bg-slate-100">
            <SettingsPanel app_ctx=app_ctx />
            <WritingStage app_ctx=app_ctx />
            <StructurePanel app_ctx=app_ctx />

            <ConfigModal is_open=app_ctx.is_config_open _app_ctx=app_ctx />
            
            <ExtractionModal
                is_open=app_ctx.is_extract_modal_open
                candidates=app_ctx.extraction_candidates
                is_extracting=app_ctx.is_extracting
            />
            
            <ConflictModal
                is_open=app_ctx.is_conflict_modal_open
                conflict_info=app_ctx.current_conflict
                on_update_setting_and_continue=move |_new_setting: String| {
                    if let Some(text) = app_ctx.pending_conflict_text.get() {
                        crate::bridge::tiptap::insert_at_cursor(&text);
                        app_ctx.save_status.set("dirty".to_string());
                        app_ctx.word_count.set(crate::bridge::tiptap::get_word_count());
                        app_ctx.pending_conflict_text.set(None);
                    }
                }
                on_rewrite_without_conflict=move || {
                    app_ctx.pending_conflict_text.set(None);
                }
            />
        </div>
    }
}

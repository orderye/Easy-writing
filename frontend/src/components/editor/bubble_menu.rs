use crate::bridge::tiptap;
use crate::components::ai::polish_modal::PolishModal;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

#[derive(Deserialize)]
struct SelectionCoords {
    top: f64,
    #[allow(dead_code)]
    bottom: f64,
    left: f64,
}

#[component]
pub fn BubbleMenu(app_ctx: AppContext) -> impl IntoView {
    let is_visible = RwSignal::new(false);
    let top_pos = RwSignal::new(0.0);
    let left_pos = RwSignal::new(0.0);
    let selected_text = RwSignal::new(String::new());

    // Polish modal state
    let is_modal_open = RwSignal::new(false);
    let polished_text = RwSignal::new(String::new());
    let is_loading = RwSignal::new(false);
    let original_cached = RwSignal::new(String::new());

    // Register selection callback once editor is mounted
    Effect::new(move |_| {
        if app_ctx.is_editor_mounted.get() {
            let cb = Closure::wrap(Box::new(move || {
                let text = tiptap::get_selection_text().unwrap_or_default();
                let coords_json = tiptap::get_selection_coords();

                if !text.trim().is_empty() {
                    if let Some(json_str) = coords_json {
                        if let Ok(c) = serde_json::from_str::<SelectionCoords>(&json_str) {
                            selected_text.set(text);
                            // Position menu right above the selection
                            let t = (c.top - 46.0).max(10.0);
                            let l = (c.left - 130.0).max(10.0);
                            top_pos.set(t);
                            left_pos.set(l);
                            is_visible.set(true);
                            return;
                        }
                    }
                }
                is_visible.set(false);
            }) as Box<dyn FnMut()>);

            tiptap::on_selection_update(cb.as_ref().unchecked_ref());
            cb.forget();
        }
    });

    let run_action = move |action_key: &'static str| {
        let text = selected_text.get();
        if text.trim().is_empty() {
            return;
        }

        original_cached.set(text.clone());
        polished_text.set(String::new());
        is_loading.set(true);
        is_modal_open.set(true);
        is_visible.set(false);

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("ai_polish")
                .arg("module_key", action_key.to_string())
                .arg("current_content", text)
                .send::<String>()
                .await;

            is_loading.set(false);
            match res {
                Ok(out) => {
                    polished_text.set(out);
                }
                Err(err) => {
                    polished_text.set(format!("AI 处理失败: {}", err));
                }
            }
        });
    };

    let on_adopt = move |new_text: String| {
        tiptap::replace_selection(&new_text);
        app_ctx.save_status.set("dirty".to_string());
        app_ctx.word_count.set(tiptap::get_word_count());
    };

    view! {
        <div>
            // Floating bubble pill
            {move || {
                if is_visible.get() && !is_modal_open.get() {
                    let top_style = format!("top: {}px; left: {}px;", top_pos.get(), left_pos.get());
                    view! {
                        <div
                            class="fixed z-40 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 shadow-xl rounded-lg px-1.5 py-1 flex items-center gap-1 text-xs select-none animate-in fade-in zoom-in-95"
                            style=top_style
                        >
                            <button
                                class="px-2 py-1 hover:bg-slate-100 dark:hover:bg-slate-700 rounded text-slate-700 dark:text-slate-200 font-medium flex items-center gap-1 transition-colors cursor-pointer"
                                title="润色选中文字"
                                on:mousedown=move |ev| {
                                    ev.prevent_default();
                                    run_action("selection_polish");
                                }
                            >
                                <span class="text-amber-500">"✨"</span>
                                <span>"润色"</span>
                            </button>
                            <button
                                class="px-2 py-1 hover:bg-slate-100 dark:hover:bg-slate-700 rounded text-slate-700 dark:text-slate-200 font-medium flex items-center gap-1 transition-colors cursor-pointer"
                                title="丰富细节与氛围"
                                on:mousedown=move |ev| {
                                    ev.prevent_default();
                                    run_action("selection_expand");
                                }
                            >
                                <span class="text-blue-500">"➕"</span>
                                <span>"扩写"</span>
                            </button>
                            <button
                                class="px-2 py-1 hover:bg-slate-100 dark:hover:bg-slate-700 rounded text-slate-700 dark:text-slate-200 font-medium flex items-center gap-1 transition-colors cursor-pointer"
                                title="精炼骨干去除冗余"
                                on:mousedown=move |ev| {
                                    ev.prevent_default();
                                    run_action("selection_compress");
                                }
                            >
                                <span class="text-emerald-500">"➖"</span>
                                <span>"缩写"</span>
                            </button>
                            <button
                                class="px-2 py-1 hover:bg-slate-100 dark:hover:bg-slate-700 rounded text-slate-700 dark:text-slate-200 font-medium flex items-center gap-1 transition-colors cursor-pointer"
                                title="根据语境承接续写"
                                on:mousedown=move |ev| {
                                    ev.prevent_default();
                                    run_action("selection_continue");
                                }
                            >
                                <span class="text-purple-500">"➡️"</span>
                                <span>"续写"</span>
                            </button>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            // Diff comparison modal for selection actions
            <PolishModal
                is_open=is_modal_open
                original_text=original_cached.get()
                polished_text=polished_text
                is_loading=is_loading
                on_adopt=on_adopt
            />
        </div>
    }
}

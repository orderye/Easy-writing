use crate::components::ai::conflict_modal::ConflictModal;
use crate::components::ai::extraction_modal::ExtractionModal;
use crate::components::config::config_modal::ConfigModal;
use crate::components::config::update_modal::UpdateModal;
use crate::components::editor::asset_gallery::AssetGalleryModal;
use crate::components::editor::revision_modal::RevisionModal;
use crate::components::layout::writing_stage::WritingStage;
use crate::components::novel::import_modal::ImportModal;
use crate::components::novel::novel_manager::NovelManagerModal;
use crate::components::settings_panel::setting_changes_modal::SettingChangesModal;
use crate::components::settings_panel::trash_modal::TrashModal;
use crate::components::settings_panel::SettingsPanel;
use crate::components::structure_panel::chapter_list::do_switch_chapter;
use crate::components::structure_panel::save_button::{perform_save, perform_save_async};
use crate::save_state;
use crate::components::structure_panel::StructurePanel;
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use shared_types::AppUpdateInfo;
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

    // 后台自动保存。
    //
    // 语义说明（重要）：这是一个**长驻循环**，不是响应式绑定。
    // auto_save_interval / save_status 都在 spawn_local 内部读取，
    // 因此改设置不会立刻重启循环，而是在下一拍轮询时生效 —— 这是刻意的，
    // 免得每次改设置都新起一个循环。
    //
    // 与旧实现的区别：
    //   1. 失败（"error"）后也会继续重试。旧实现只看 "dirty"，
    //      一旦进过 error 状态自动保存就永久停摆（磁盘满、库被锁都会走到那里）；
    //   2. 用 perform_save_async 真正 await 保存结束再判成败，
    //      不再靠固定 sleep 猜（慢请求会被误判为失败）；
    //   3. 连续失败达阈值后给出常驻告警，而不是只在按钮上变个颜色。
    Effect::new(move |_| {
        let ctx = app_ctx;
        leptos::task::spawn_local(async move {
            let mut failures: u32 = 0;
            loop {
                let wait_secs = save_state::auto_save_backoff_secs(
                    ctx.auto_save_interval.get_untracked(),
                    failures,
                );
                TimeoutFuture::new(wait_secs * 1000).await;

                match ctx.save_status.get_untracked().as_str() {
                    "dirty" | "error" => {
                        let ok = perform_save_async(ctx).await;
                        if ok {
                            failures = 0;
                            ctx.save_alert.set(None);
                        } else {
                            failures += 1;
                            if failures >= 3 {
                                ctx.save_alert.set(Some(
                                    "自动保存已连续失败，请检查磁盘空间后手动保存（⌘S）".to_string(),
                                ));
                            }
                        }
                    }
                    _ => {
                        failures = 0;
                    }
                }
            }
        });
    });

    // Background automatic update check on application launch (delayed 4 seconds)
    Effect::new(move |_| {
        let ctx = app_ctx;
        leptos::task::spawn_local(async move {
            TimeoutFuture::new(4000).await;
            let is_auto_enabled = if let Ok(Some(val)) = invoke_cmd("get_app_setting")
                .arg("key", "auto_check_update".to_string())
                .send::<Option<String>>()
                .await
            {
                val != "false"
            } else {
                true
            };

            if is_auto_enabled {
                if let Ok(info) = invoke_cmd("check_app_update")
                    .arg("force", false)
                    .send::<AppUpdateInfo>()
                    .await
                {
                    if info.has_update {
                        ctx.available_update.set(Some(info));
                        ctx.has_update_badge.set(true);
                        ctx.is_update_modal_open.set(true);
                    }
                }
            }
        });
    });

    view! {
        <div class="h-full w-full flex flex-row overflow-hidden bg-slate-100 dark:bg-slate-900">
            <SettingsPanel app_ctx=app_ctx />
            <WritingStage app_ctx=app_ctx />
            <StructurePanel app_ctx=app_ctx />

            <ConfigModal is_open=app_ctx.is_config_open app_ctx=app_ctx />

            <UpdateModal is_open=app_ctx.is_update_modal_open app_ctx=app_ctx />

            <RevisionModal is_open=app_ctx.is_revisions_modal_open app_ctx=app_ctx />

            <AssetGalleryModal app_ctx=app_ctx />

            <SettingChangesModal is_open=app_ctx.is_setting_changes_modal_open app_ctx=app_ctx />

            <TrashModal is_open=app_ctx.is_trash_modal_open app_ctx=app_ctx />
            <NovelManagerModal app_ctx=app_ctx />

            <ImportModal is_open=app_ctx.is_import_modal_open app_ctx=app_ctx />

            <ExtractionModal
                app_ctx=app_ctx
                is_open=app_ctx.is_extract_modal_open
                candidates=app_ctx.extraction_candidates
                is_extracting=app_ctx.is_extracting
            />

            <ConflictModal
                is_open=app_ctx.is_conflict_modal_open
                conflict_info=app_ctx.current_conflict
                on_update_setting_and_continue=move |new_setting: String| {
                    if let (Some(novel), Some(conflict)) = (app_ctx.current_novel.get(), app_ctx.current_conflict.get()) {
                        let nid = novel.id;
                        let cid = app_ctx.current_chapter.get().map(|c| c.id);
                        let mod_name = conflict.setting_module.clone();
                        let title = conflict.setting_title.clone();
                        let reason = conflict.reason.clone();
                        let new_val = new_setting.clone();
                        leptos::task::spawn_local(async move {
                            let _ = invoke_cmd("apply_conflict_setting_update")
                                .arg("novel_id", nid)
                                .opt("chapter_id", cid)
                                .arg("setting_module", mod_name)
                                .arg("setting_title", title)
                                .arg("new_content", new_val)
                                .arg("reason", format!("AI冲突消除覆盖: {}", reason))
                                .send_unit()
                                .await;
                        });
                    }
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
                on_force_apply=move || {
                    if let Some(text) = app_ctx.pending_conflict_text.get() {
                        crate::bridge::tiptap::insert_at_cursor(&text);
                        app_ctx.save_status.set("dirty".to_string());
                        app_ctx.word_count.set(crate::bridge::tiptap::get_word_count());
                        app_ctx.pending_conflict_text.set(None);
                    }
                }
            />

            // Unsaved changes confirm modal
            {move || {
                if app_ctx.is_unsaved_modal_open.get() {
                    let target_title = app_ctx
                        .pending_switch_chapter
                        .get()
                        .map(|c| c.title)
                        .unwrap_or_else(|| "目标章节".to_string());

                    let target_id = app_ctx
                        .pending_switch_chapter
                        .get()
                        .map(|c| c.id)
                        .unwrap_or_default();

                    let target_id_save = target_id.clone();
                    let target_id_discard = target_id.clone();

                    view! {
                        <div class="fixed inset-0 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center z-50 p-6 select-none animate-in fade-in">
                            <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-md w-full border border-slate-200 dark:border-slate-700 p-6 space-y-4">
                                <div class="flex items-center gap-3 text-amber-600 dark:text-amber-400">
                                    <span class="text-2xl">"⚠️"</span>
                                    <h3 class="font-bold text-base text-slate-800 dark:text-slate-100">"未保存修改提示"</h3>
                                </div>
                                <p class="text-xs text-slate-600 dark:text-slate-300 leading-relaxed">
                                    "当前章节有尚未保存的正文内容。切换至「" <span class="font-bold text-slate-800 dark:text-slate-100">{target_title}</span> "」前，请选择处理方式："
                                </p>
                                <div class="flex items-center justify-end gap-2 pt-2 text-xs">
                                    <button
                                        class="px-3 py-1.5 border border-slate-300 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 rounded transition-colors cursor-pointer"
                                        on:click=move |_| {
                                            app_ctx.pending_switch_chapter.set(None);
                                            app_ctx.is_unsaved_modal_open.set(false);
                                        }
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="px-3 py-1.5 border border-red-200 dark:border-red-900/60 bg-red-50 dark:bg-red-950/40 text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/60 rounded transition-colors cursor-pointer"
                                        on:click={
                                            let tid = target_id_discard.clone();
                                            move |_| {
                                                app_ctx.is_unsaved_modal_open.set(false);
                                                app_ctx.pending_switch_chapter.set(None);
                                                do_switch_chapter(app_ctx, tid.clone());
                                            }
                                        }
                                    >
                                        "放弃修改"
                                    </button>
                                    <button
                                        class="px-4 py-1.5 bg-blue-600 hover:bg-blue-700 text-white font-medium rounded shadow-xs transition-colors cursor-pointer"
                                        on:click={
                                            let tid = target_id_save.clone();
                                            move |_| {
                                                let tid = tid.clone();
                                                leptos::task::spawn_local(async move {
                                                    app_ctx.is_unsaved_modal_open.set(false);
                                                    let ok = crate::components::structure_panel::save_button::perform_save_async(app_ctx).await;
                                                    if ok {
                                                        app_ctx.pending_switch_chapter.set(None);
                                                        do_switch_chapter(app_ctx, tid);
                                                    } else {
                                                        web_sys::console::error_1(&"切章前保存失败，已拦截切章操作以防丢稿".into());
                                                    }
                                                });
                                            }
                                        }
                                    >
                                        "保存并切换"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </div>
    }
}

use crate::components::ai::chat_box::ChatBox;
use crate::components::editor::bubble_menu::BubbleMenu;
use crate::components::editor::chapter_nav::ChapterNav;
use crate::components::editor::editor_wrapper::EditorWrapper;
use crate::components::editor::status_bar::StatusBar;
use crate::components::editor::toolbar::EditorToolbar;
use crate::state::AppContext;
use leptos::prelude::*;
use shared_types::Volume;

#[component]
pub fn WritingStage(app_ctx: AppContext) -> impl IntoView {
    let has_chapter = move || app_ctx.current_chapter.get().is_some();

    let on_create_chapter = move |_| {
        if let Some(vol) = app_ctx.current_volume.get() {
            crate::components::structure_panel::chapter_list::create_chapter_in_volume(app_ctx, vol.id);
        } else if let Some(first_vol) = app_ctx.volumes_list.get().first() {
            crate::components::structure_panel::chapter_list::create_chapter_in_volume(app_ctx, first_vol.id.clone());
        } else if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                if let Ok(new_vol) = crate::tauri_api::invoke_cmd("create_volume")
                    .arg("novel_id", novel_id.clone())
                    .arg("title", "第一卷".to_string())
                    .arg("summary", "".to_string())
                    .send::<Volume>()
                    .await
                {
                    let vol_id = new_vol.id.clone();
                    app_ctx.volumes_list.update(|v| v.push(new_vol.clone()));
                    app_ctx.current_volume.set(Some(new_vol));
                    crate::components::structure_panel::chapter_list::create_chapter_in_volume(app_ctx, vol_id);
                }
            });
        }
    };

    view! {
        <main class="flex-1 min-h-0 flex flex-col h-full bg-white dark:bg-slate-900 overflow-hidden relative">
            <ChapterNav app_ctx=app_ctx />
            <EditorToolbar app_ctx=app_ctx />
            <div class="flex-1 min-h-0 relative flex flex-col">
                <div class=move || if has_chapter() { "flex-1 min-h-0 flex flex-col h-full" } else { "hidden" }>
                    <EditorWrapper app_ctx=app_ctx />
                </div>
                {move || {
                    if !has_chapter() {
                        Some(view! {
                            <div class="flex-1 min-h-0 flex flex-col items-center justify-center text-center p-8 bg-slate-50/50 dark:bg-slate-900/60 select-none">
                                <div class="w-16 h-16 rounded-2xl bg-amber-500/10 text-amber-500 flex items-center justify-center text-3xl mb-4 border border-amber-500/20 shadow-sm">
                                    "📝"
                                </div>
                                <h3 class="text-base font-semibold text-slate-800 dark:text-slate-200 mb-1.5">
                                    "当前没有打开的章节"
                                </h3>
                                <p class="text-xs text-slate-500 dark:text-slate-400 mb-5 max-w-sm leading-relaxed">
                                    "作品中暂无可编辑的章节。请在左侧目录面板中选择章节，或点击下方按钮新建章节开始创作。"
                                </p>
                                <button
                                    type="button"
                                    class="px-4 py-2 text-xs font-medium rounded-lg bg-amber-600 hover:bg-amber-500 text-white shadow-sm transition-colors cursor-pointer flex items-center gap-1.5"
                                    on:click=on_create_chapter
                                >
                                    <span>"＋"</span>
                                    <span>"新建章节"</span>
                                </button>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
            </div>
            <BubbleMenu app_ctx=app_ctx />
            <ChatBox app_ctx=app_ctx />
            <StatusBar app_ctx=app_ctx />
        </main>
    }
}


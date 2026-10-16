use crate::components::ai::chat_box::ChatBox;
use crate::components::editor::bubble_menu::BubbleMenu;
use crate::components::editor::chapter_nav::ChapterNav;
use crate::components::editor::editor_wrapper::EditorWrapper;
use crate::components::editor::status_bar::StatusBar;
use crate::components::editor::toolbar::EditorToolbar;
use crate::state::AppContext;
use leptos::prelude::*;

#[component]
pub fn WritingStage(app_ctx: AppContext) -> impl IntoView {
    view! {
        <main class="flex-1 min-h-0 flex flex-col h-full bg-white dark:bg-slate-900 overflow-hidden relative">
            <ChapterNav app_ctx=app_ctx />
            <EditorToolbar app_ctx=app_ctx />
            <EditorWrapper app_ctx=app_ctx />
            <BubbleMenu app_ctx=app_ctx />
            <ChatBox app_ctx=app_ctx />
            <StatusBar app_ctx=app_ctx />
        </main>
    }
}

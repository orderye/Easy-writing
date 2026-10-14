pub mod characters;
pub mod outline;
pub mod plot_lines;
pub mod setting_changes_modal;
pub mod trash_modal;
pub mod worldview;


use crate::components::layout::panel_toggle::LeftToggle;
use crate::components::settings_panel::characters::CharactersView;
use crate::components::settings_panel::outline::OutlineView;
use crate::components::settings_panel::plot_lines::PlotLinesView;
use crate::components::settings_panel::worldview::WorldviewView;
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct UpdateNovelArgs {
    id: String,
    title: String,
    synopsis: String,
}

#[derive(Serialize)]
struct SetAppSettingArgs {
    key: String,
    value: String,
}

#[component]
pub fn SettingsPanel(app_ctx: AppContext) -> impl IntoView {
    let novel_title = move || {
        app_ctx
            .current_novel
            .get()
            .map(|n| n.title)
            .unwrap_or_else(|| "未命名作品".to_string())
    };

    let active_tab = move || app_ctx.active_settings_tab.get();

    let on_update_title = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        if let Some(mut novel) = app_ctx.current_novel.get() {
            novel.title = val.clone();
            app_ctx.current_novel.set(Some(novel.clone()));
            leptos::task::spawn_local(async move {
                let _ = invoke::<(), _>(
                    "update_novel",
                    &UpdateNovelArgs {
                        id: novel.id,
                        title: val,
                        synopsis: novel.synopsis,
                    },
                )
                .await;
            });
        }
    };

    view! {
        <aside
            class=move || {
                format!(
                    "h-full bg-slate-50 dark:bg-slate-900 border-r border-slate-200 dark:border-slate-800 flex flex-col panel-transition select-none shrink-0 {}",
                    if app_ctx.left_panel_open.get() { "w-72" } else { "w-0 overflow-hidden border-none" }
                )
            }
        >
            // Header with novel title and collapse button
            <div class="h-12 border-b border-slate-200 dark:border-slate-700 px-3 flex items-center justify-between shrink-0">
                <input
                    type="text"
                    class="font-bold text-slate-800 dark:text-slate-100 text-sm bg-transparent hover:bg-slate-200/60 dark:hover:bg-slate-800/80 px-1.5 py-0.5 rounded focus:bg-white dark:focus:bg-slate-800 focus:outline-blue-500 w-44 truncate"
                    title="点击修改作品名称"
                    prop:value=novel_title
                    on:change=on_update_title
                />
                <LeftToggle is_open=app_ctx.left_panel_open />
            </div>

            // Four Settings Tabs
            <div class="grid grid-cols-4 border-b border-slate-200 dark:border-slate-700 bg-slate-100 dark:bg-slate-800 text-xs text-slate-600 dark:text-slate-400 font-medium text-center shrink-0">
                <button
                    class=move || {
                        format!(
                            "py-2 hover:bg-white dark:hover:bg-slate-700 transition-colors border-b-2 cursor-pointer {}",
                            if active_tab() == "worldview" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-900 font-bold" } else { "border-transparent" }
                        )
                    }
                    on:click=move |_| app_ctx.active_settings_tab.set("worldview".to_string())
                >
                    "世界观"
                </button>
                <button
                    class=move || {
                        format!(
                            "py-2 hover:bg-white dark:hover:bg-slate-700 transition-colors border-b-2 cursor-pointer {}",
                            if active_tab() == "character" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-900 font-bold" } else { "border-transparent" }
                        )
                    }
                    on:click=move |_| app_ctx.active_settings_tab.set("character".to_string())
                >
                    "角色"
                </button>
                <button
                    class=move || {
                        format!(
                            "py-2 hover:bg-white dark:hover:bg-slate-700 transition-colors border-b-2 cursor-pointer {}",
                            if active_tab() == "plot" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-900 font-bold" } else { "border-transparent" }
                        )
                    }
                    on:click=move |_| app_ctx.active_settings_tab.set("plot".to_string())
                >
                    "剧情"
                </button>
                <button
                    class=move || {
                        format!(
                            "py-2 hover:bg-white dark:hover:bg-slate-700 transition-colors border-b-2 cursor-pointer {}",
                            if active_tab() == "outline" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-900 font-bold" } else { "border-transparent" }
                        )
                    }
                    on:click=move |_| app_ctx.active_settings_tab.set("outline".to_string())
                >
                    "大纲"
                </button>
            </div>

            // Settings Search Bar
            <div class="px-2.5 py-1.5 border-b border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 shrink-0">
                <div class="relative flex items-center">
                    <span class="absolute left-2 text-slate-400 text-xs">"🔍"</span>
                    <input
                        type="text"
                        class="w-full pl-6 pr-6 py-1 text-xs rounded-md bg-slate-100 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-slate-800 dark:text-slate-100 placeholder-slate-400 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                        placeholder="搜索设定、角色、剧情..."
                        prop:value=move || app_ctx.settings_search_query.get()
                        on:input=move |ev| app_ctx.settings_search_query.set(event_target_value(&ev))
                    />
                    {move || {
                        if !app_ctx.settings_search_query.get().is_empty() {
                            view! {
                                <button
                                    class="absolute right-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 text-xs cursor-pointer"
                                    on:click=move |_| app_ctx.settings_search_query.set(String::new())
                                >
                                    "✕"
                                </button>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }
                    }}
                </div>
            </div>

            // Active Tab Content
            <div class="flex-1 min-h-0 overflow-hidden flex flex-col bg-white dark:bg-slate-900">
                {move || match active_tab().as_str() {
                    "character" => view! { <CharactersView app_ctx=app_ctx /> }.into_any(),
                    "plot" => view! { <PlotLinesView app_ctx=app_ctx /> }.into_any(),
                    "outline" => view! { <OutlineView app_ctx=app_ctx /> }.into_any(),
                    _ => view! { <WorldviewView app_ctx=app_ctx /> }.into_any(),
                }}
            </div>

            // Footer Settings & Quick Theme Toggle
            <div class="h-10 border-t border-slate-200 dark:border-slate-700 px-3 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 bg-slate-50 dark:bg-slate-900 shrink-0">
                <div class="flex items-center gap-2">
                    <button
                        class="flex items-center gap-1.5 hover:text-slate-800 dark:hover:text-slate-200 transition-colors p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-800 cursor-pointer"
                        title="打开系统设置 (模型接口、WebDAV 等)"
                        on:click=move |_| app_ctx.is_config_open.set(true)
                    >
                        <span>"⚙️"</span>
                        <span>"设置"</span>
                    </button>
                    <button
                        class="flex items-center gap-1 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-1.5 py-0.5 rounded hover:bg-slate-200 dark:hover:bg-slate-800 cursor-pointer text-[11px] border border-slate-200 dark:border-slate-700"
                        title="查看世界观/角色/剧情历史设定改动留档与一键撤销"
                        on:click=move |_| app_ctx.is_setting_changes_modal_open.set(true)
                    >
                        <span>"📜"</span>
                        <span>"留档"</span>
                    </button>
                    <button
                        class="flex items-center gap-1 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-1.5 py-0.5 rounded hover:bg-slate-200 dark:hover:bg-slate-800 cursor-pointer text-[11px] border border-slate-200 dark:border-slate-700"
                        title="打开卷章与设定废弃回收站"
                        on:click=move |_| app_ctx.is_trash_modal_open.set(true)
                    >
                        <span>"🗑️"</span>
                        <span>"回收站"</span>
                    </button>
                    <button
                        class="flex items-center gap-1 hover:text-blue-600 dark:hover:text-blue-400 transition-colors px-1.5 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 cursor-pointer text-[11px] border border-blue-200 dark:border-blue-800/80 font-medium text-blue-600 dark:text-blue-400"
                        title="批量导入外部 txt/md 文稿并智能拆卷拆章"
                        on:click=move |_| app_ctx.is_import_modal_open.set(true)
                    >
                        <span>"📥"</span>
                        <span>"导入"</span>
                    </button>
                    <button
                        class="flex items-center gap-1 hover:text-slate-800 dark:hover:text-slate-200 transition-colors px-1.5 py-0.5 rounded hover:bg-slate-200 dark:hover:bg-slate-800 cursor-pointer text-[11px] border border-slate-200 dark:border-slate-700"
                        title="快速切换浅色 / 深色 / 跟随系统模式"
                        on:click=move |_| {
                            let next_theme = match app_ctx.theme.get().as_str() {
                                "dark" => "light",
                                "light" => "system",
                                _ => "dark",
                            };
                            app_ctx.theme.set(next_theme.to_string());
                            crate::bridge::theme::apply_app_theme(next_theme);
                            let nt = next_theme.to_string();
                            leptos::task::spawn_local(async move {
                                let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                                    key: "theme".to_string(),
                                    value: nt,
                                }).await;
                            });
                        }
                    >
                        {move || match app_ctx.theme.get().as_str() {
                            "dark" => "🌙 暗色",
                            "light" => "☀️ 亮色",
                            _ => "🌓 系统",
                        }}
                    </button>
                </div>
                <span class="text-[10px] text-slate-400 dark:text-slate-500">"v0.1.0"</span>
            </div>
        </aside>
    }
}

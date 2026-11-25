use crate::components::icons::{
    IconBookOpen, IconDownload, IconHistory, IconImage,
    IconMaximize, IconSave, IconSearch, IconSettings, IconSparkles, IconZap,
};
use crate::components::structure_panel::save_button::perform_save;
use crate::state::AppContext;
use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct CommandItem {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub shortcut: Option<&'static str>,
}

pub const ALL_COMMANDS: &[CommandItem] = &[
    CommandItem {
        id: "save",
        title: "立即保存当前章节",
        category: "创作",
        shortcut: Some("⌘S"),
    },
    CommandItem {
        id: "search",
        title: "全文搜索与批量替换",
        category: "导航",
        shortcut: Some("⌘F"),
    },
    CommandItem {
        id: "focus_mode",
        title: "切换专注模式（隐藏左右两栏）",
        category: "视图",
        shortcut: Some("⌘⇧F"),
    },
    CommandItem {
        id: "typewriter_mode",
        title: "切换打字机模式（光标行垂直居中）",
        category: "视图",
        shortcut: Some("⌘⇧T"),
    },
    CommandItem {
        id: "spotlight_mode",
        title: "切换段落聚光灯模式（淡化非编辑段落）",
        category: "视图",
        shortcut: None,
    },
    CommandItem {
        id: "export",
        title: "导出作品（TXT / Word DOCX / EPUB）",
        category: "发布",
        shortcut: None,
    },
    CommandItem {
        id: "gallery",
        title: "打开本地资产图库",
        category: "资产",
        shortcut: None,
    },
    CommandItem {
        id: "foreshadow",
        title: "打开伏笔看板与回收追踪",
        category: "设定",
        shortcut: None,
    },
    CommandItem {
        id: "revisions",
        title: "查看章节版本历史与时间轴",
        category: "安全",
        shortcut: None,
    },
    CommandItem {
        id: "extract",
        title: "AI 提炼设定（世界观/人物/剧情线）",
        category: "AI 助手",
        shortcut: None,
    },
    CommandItem {
        id: "config",
        title: "系统设置（大模型 / WebDAV 备份 / 偏好）",
        category: "系统",
        shortcut: None,
    },
    CommandItem {
        id: "theme_light",
        title: "切换为「皓月雅白」主题",
        category: "主题",
        shortcut: None,
    },
    CommandItem {
        id: "theme_sepia",
        title: "切换为「水墨宣纸」护眼主题",
        category: "主题",
        shortcut: None,
    },
    CommandItem {
        id: "theme_dark",
        title: "切换为「墨石深灰」暗黑主题",
        category: "主题",
        shortcut: None,
    },
];

#[component]
pub fn CommandPalette(app_ctx: AppContext) -> impl IntoView {
    let is_open = move || app_ctx.is_command_palette_open.get();
    let query = RwSignal::new(String::new());
    let selected_index = RwSignal::new(0usize);

    let filtered_commands = move || -> Vec<CommandItem> {
        let q = query.get().trim().to_lowercase();
        if q.is_empty() {
            ALL_COMMANDS.to_vec()
        } else {
            ALL_COMMANDS
                .iter()
                .filter(|item| {
                    item.title.to_lowercase().contains(&q)
                        || item.category.to_lowercase().contains(&q)
                })
                .copied()
                .collect()
        }
    };

    Effect::new(move |_| {
        if app_ctx.is_command_palette_open.get() {
            selected_index.set(0);
        }
    });

    let persist_theme = move |theme_val: &'static str| {
        app_ctx.theme.set(theme_val.to_string());
        crate::bridge::theme::apply_app_theme(theme_val);
        leptos::task::spawn_local(async move {
            let _ = crate::tauri_api::invoke_cmd("set_app_setting")
                .arg("key", "theme".to_string())
                .arg("value", theme_val.to_string())
                .send_unit()
                .await;
        });
    };

    let on_execute = move |id: &'static str| {
        app_ctx.is_command_palette_open.set(false);
        match id {
            "save" => perform_save(app_ctx),
            "search" => app_ctx.is_search_open.set(true),
            "focus_mode" => app_ctx.is_focus_mode.update(|v| *v = !*v),
            "typewriter_mode" => app_ctx.is_typewriter_mode.update(|v| *v = !*v),
            "spotlight_mode" => app_ctx.is_spotlight_mode.update(|v| *v = !*v),
            "export" => app_ctx.is_export_modal_open.set(true),
            "gallery" => app_ctx.is_asset_gallery_open.set(true),
            "foreshadow" => {
                app_ctx.active_settings_tab.set("foreshadow".to_string());
                app_ctx.left_panel_open.set(true);
            }
            "revisions" => {
                if app_ctx.current_chapter.get().is_some() {
                    app_ctx.is_revisions_modal_open.set(true);
                }
            }
            "extract" => app_ctx.is_extract_modal_open.set(true),
            "config" => app_ctx.is_config_open.set(true),
            "theme_light" => persist_theme("light"),
            "theme_sepia" => persist_theme("sepia"),
            "theme_dark" => persist_theme("dark"),
            _ => {}
        }
    };

    view! {
        {move || {
            if is_open() {
                Some(view! {
                    <div
                        class="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-slate-900/40 backdrop-blur-xs select-none"
                        on:click=move |_| app_ctx.is_command_palette_open.set(false)
                    >
                        <div
                            class="w-full max-w-xl bg-white dark:bg-slate-900 rounded-xl shadow-2xl border border-slate-200/80 dark:border-slate-800 overflow-hidden flex flex-col"
                            on:click=move |e| e.stop_propagation()
                        >
                            // 输入框（不随 query 变化销毁重建，保障中文输入法与焦点稳定）
                            <div class="flex items-center px-4 py-3 border-b border-slate-200 dark:border-slate-800 gap-3">
                                <IconSearch class="w-5 h-5 text-slate-400" />
                                <input
                                    type="text"
                                    class="flex-1 bg-transparent text-sm text-slate-900 dark:text-slate-100 placeholder-slate-400 focus:outline-none"
                                    placeholder="输入指令、功能或设定名称快速直达 (Esc 退出)..."
                                    autofocus=true
                                    prop:value=move || query.get()
                                    on:input=move |e| {
                                        query.set(event_target_value(&e));
                                        selected_index.set(0);
                                    }
                                    on:keydown=move |e: web_sys::KeyboardEvent| {
                                        let list = filtered_commands();
                                        if list.is_empty() {
                                            return;
                                        }
                                        match e.key().as_str() {
                                            "ArrowDown" => {
                                                e.prevent_default();
                                                selected_index.update(|idx| *idx = (*idx + 1) % list.len());
                                            }
                                            "ArrowUp" => {
                                                e.prevent_default();
                                                selected_index.update(|idx| {
                                                    *idx = if *idx == 0 { list.len() - 1 } else { *idx - 1 };
                                                });
                                            }
                                            "Enter" => {
                                                e.prevent_default();
                                                let idx = selected_index.get_untracked().min(list.len() - 1);
                                                on_execute(list[idx].id);
                                            }
                                            _ => {}
                                        }
                                    }
                                />
                                <kbd class="px-2 py-0.5 text-[10px] font-mono text-slate-400 bg-slate-100 dark:bg-slate-800 rounded border border-slate-200 dark:border-slate-700">
                                    "ESC"
                                </kbd>
                            </div>

                            // 命令列表
                            <div class="max-h-80 overflow-y-auto p-2 space-y-1">
                                {move || {
                                    let list = filtered_commands();
                                    let active_idx = selected_index.get();
                                    if list.is_empty() {
                                        view! {
                                            <div class="py-8 text-center text-xs text-slate-400">
                                                "未找到匹配的命令"
                                            </div>
                                        }.into_any()
                                    } else {
                                        list.into_iter().enumerate().map(|(idx, cmd)| {
                                            let cmd_id = cmd.id;
                                            let is_selected = idx == active_idx;
                                            let icon = match cmd.id {
                                                "save" => view! { <IconSave class="w-4 h-4 text-emerald-500" /> }.into_any(),
                                                "search" => view! { <IconSearch class="w-4 h-4 text-blue-500" /> }.into_any(),
                                                "focus_mode" | "typewriter_mode" | "spotlight_mode" => view! { <IconMaximize class="w-4 h-4 text-indigo-500" /> }.into_any(),
                                                "export" => view! { <IconDownload class="w-4 h-4 text-amber-500" /> }.into_any(),
                                                "gallery" => view! { <IconImage class="w-4 h-4 text-purple-500" /> }.into_any(),
                                                "foreshadow" => view! { <IconBookOpen class="w-4 h-4 text-rose-500" /> }.into_any(),
                                                "revisions" => view! { <IconHistory class="w-4 h-4 text-cyan-500" /> }.into_any(),
                                                "extract" => view! { <IconSparkles class="w-4 h-4 text-violet-500" /> }.into_any(),
                                                "config" => view! { <IconSettings class="w-4 h-4 text-slate-500" /> }.into_any(),
                                                _ => view! { <IconZap class="w-4 h-4 text-amber-500" /> }.into_any(),
                                            };

                                            view! {
                                                <button
                                                    class=if is_selected {
                                                        "w-full px-3 py-2 text-left rounded-lg bg-slate-100 dark:bg-slate-800 transition-colors flex items-center justify-between group cursor-pointer"
                                                    } else {
                                                        "w-full px-3 py-2 text-left rounded-lg hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors flex items-center justify-between group cursor-pointer"
                                                    }
                                                    on:mouseenter=move |_| selected_index.set(idx)
                                                    on:click=move |_| on_execute(cmd_id)
                                                >
                                                    <div class="flex items-center gap-3">
                                                        <div class="p-1 rounded bg-slate-50 dark:bg-slate-800/80 border border-slate-200/60 dark:border-slate-700/60">
                                                            {icon}
                                                        </div>
                                                        <div class="flex flex-col">
                                                            <span class="text-xs font-medium text-slate-800 dark:text-slate-200 group-hover:text-blue-600 dark:group-hover:text-blue-400">
                                                                {cmd.title}
                                                            </span>
                                                            <span class="text-[10px] text-slate-400">
                                                                {cmd.category}
                                                            </span>
                                                        </div>
                                                    </div>
                                                    {cmd.shortcut.map(|sc| view! {
                                                        <kbd class="px-1.5 py-0.5 text-[10px] font-mono text-slate-400 group-hover:text-slate-600 dark:group-hover:text-slate-200 bg-slate-100 dark:bg-slate-800 rounded border border-slate-200 dark:border-slate-700">
                                                            {sc}
                                                        </kbd>
                                                    })}
                                                </button>
                                            }
                                        }).collect::<Vec<_>>().into_any()
                                    }
                                }}
                            </div>

                            // 底部状态条
                            <div class="px-4 py-2 bg-slate-50 dark:bg-slate-950/40 border-t border-slate-200/60 dark:border-slate-800/60 flex items-center justify-between text-[11px] text-slate-400">
                                <span>"↑↓ 移动 · 按 ↵ 确认选择"</span>
                                <span>"⌘K 随时唤起"</span>
                            </div>
                        </div>
                    </div>
                })
            } else {
                None
            }
        }}
    }
}

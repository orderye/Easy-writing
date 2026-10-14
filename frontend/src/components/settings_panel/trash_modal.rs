use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::TrashItem;

#[derive(Serialize)]
struct ListTrashArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct RestoreTrashArgs {
    item_type: String,
    item_id: String,
}

#[derive(Serialize)]
struct PurgeTrashArgs {
    item_type: String,
    item_id: String,
}

#[derive(Serialize)]
struct EmptyTrashArgs {
    novel_id: String,
}

#[derive(Serialize)]
struct ListVolumesArgs {
    novel_id: String,
}

#[component]
pub fn TrashModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let items = RwSignal::new(Vec::<TrashItem>::new());
    let is_loading = RwSignal::new(false);
    let operating_id = RwSignal::new(Option::<String>::None);
    let filter_type = RwSignal::new("all".to_string());
    let show_confirm_empty = RwSignal::new(false);

    let load_trash = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let nid = novel.id;
            is_loading.set(true);
            leptos::task::spawn_local(async move {
                if let Ok(list) = invoke::<Vec<TrashItem>, _>(
                    "list_trash_items",
                    &ListTrashArgs { novel_id: nid },
                )
                .await
                {
                    items.set(list);
                }
                is_loading.set(false);
            });
        }
    };

    Effect::new(move |_| {
        if is_open.get() {
            load_trash();
        }
    });

    let refresh_structure_tree = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let nid = novel.id;
            leptos::task::spawn_local(async move {
                if let Ok(vols) = invoke::<Vec<shared_types::Volume>, _>(
                    "list_volumes",
                    &ListVolumesArgs { novel_id: nid },
                )
                .await
                {
                    app_ctx.volumes_list.set(vols);
                }
            });
        }
    };

    let handle_restore = move |item_type: String, item_id: String| {
        operating_id.set(Some(item_id.clone()));
        let t = item_type.clone();
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>(
                "restore_trash_item",
                &RestoreTrashArgs {
                    item_type: item_type.clone(),
                    item_id,
                },
            )
            .await;
            operating_id.set(None);
            load_trash();
            if t == "volume" || t == "chapter" {
                refresh_structure_tree();
            }
        });
    };

    let handle_purge = move |item_type: String, item_id: String| {
        operating_id.set(Some(item_id.clone()));
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>(
                "purge_trash_item",
                &PurgeTrashArgs {
                    item_type,
                    item_id,
                },
            )
            .await;
            operating_id.set(None);
            load_trash();
        });
    };

    let handle_empty_trash = move || {
        if let Some(novel) = app_ctx.current_novel.get() {
            let nid = novel.id;
            leptos::task::spawn_local(async move {
                let _ = invoke::<(), _>(
                    "empty_trash",
                    &EmptyTrashArgs { novel_id: nid },
                )
                .await;
                show_confirm_empty.set(false);
                load_trash();
            });
        }
    };

    let get_filtered_items = move || {
        let list = items.get();
        let f = filter_type.get();
        if f == "all" {
            list
        } else {
            list.into_iter().filter(|i| i.item_type == f).collect()
        }
    };

    view! {
        {move || {
            if !is_open.get() {
                return view! { <div class="hidden"></div> }.into_any();
            }

            view! {
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-6 select-none animate-in fade-in duration-200">
                    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-xl shadow-2xl w-full max-w-4xl h-[85vh] flex flex-col overflow-hidden text-slate-800 dark:text-slate-100">
                        // Header
                        <div class="px-6 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50 dark:bg-slate-800/50">
                            <div class="flex items-center gap-3">
                                <span class="text-xl">"🗑️"</span>
                                <div>
                                    <h2 class="text-base font-bold text-slate-800 dark:text-slate-100">
                                        "回收站 (Recycle Bin)"
                                    </h2>
                                    <p class="text-xs text-slate-500 dark:text-slate-400">
                                        "误删的卷、章、世界观、角色和剧情均可在此完整找回或彻底抹除。"
                                    </p>
                                </div>
                            </div>
                            <button
                                class="p-1 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-lg cursor-pointer"
                                title="关闭"
                                on:click=move |_| is_open.set(false)
                            >
                                "✕"
                            </button>
                        </div>

                        // Filter Toolbar
                        <div class="px-6 py-2.5 bg-slate-100/70 dark:bg-slate-800/30 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
                            <div class="flex items-center gap-2">
                                <span class="text-xs font-semibold text-slate-500 dark:text-slate-400 mr-1">"类型筛选:"</span>
                                {[("all", "全部"), ("volume", "卷目"), ("chapter", "章节"), ("worldview", "世界观"), ("character", "角色"), ("plot", "剧情")].into_iter().map(|(key, label)| {
                                    let is_active = move || filter_type.get() == key;
                                    view! {
                                        <button
                                            class=move || {
                                                format!(
                                                    "px-2.5 py-1 text-xs rounded-md transition-colors cursor-pointer {}",
                                                    if is_active() {
                                                        "bg-white dark:bg-slate-800 font-bold text-blue-600 dark:text-blue-400 shadow-xs border border-slate-300 dark:border-slate-700"
                                                    } else {
                                                        "text-slate-600 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-slate-800/60"
                                                    }
                                                )
                                            }
                                            on:click=move |_| filter_type.set(key.to_string())
                                        >
                                            {label}
                                        </button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>

                            <div class="flex items-center gap-3">
                                <span class="text-xs text-slate-400">
                                    {move || format!("共 {} 项已废弃", get_filtered_items().len())}
                                </span>

                                {move || {
                                    if items.get().is_empty() {
                                        view! { <span></span> }.into_any()
                                    } else {
                                        view! {
                                            <button
                                                class="px-2.5 py-1 text-xs rounded-md font-semibold text-rose-600 dark:text-rose-400 hover:bg-rose-50 dark:hover:bg-rose-950/40 border border-rose-200 dark:border-rose-900/60 transition-colors cursor-pointer"
                                                on:click=move |_| show_confirm_empty.set(true)
                                            >
                                                "清空回收站"
                                            </button>
                                        }.into_any()
                                    }
                                }}
                            </div>
                        </div>

                        // Confirm Empty Modal Dialog
                        {move || {
                            if show_confirm_empty.get() {
                                view! {
                                    <div class="px-6 py-3 bg-rose-50 dark:bg-rose-950/40 border-b border-rose-200 dark:border-rose-900/50 flex items-center justify-between animate-in fade-in">
                                        <div class="flex items-center gap-2 text-rose-800 dark:text-rose-200 text-xs font-semibold">
                                            <span>"⚠️"</span>
                                            <span>"确定要彻底清空回收站吗？此操作无法撤销，所有回收站内容将被物理删除！"</span>
                                        </div>
                                        <div class="flex items-center gap-2">
                                            <button
                                                class="px-3 py-1 text-xs rounded bg-slate-200 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-medium hover:bg-slate-300 cursor-pointer"
                                                on:click=move |_| show_confirm_empty.set(false)
                                            >
                                                "取消"
                                            </button>
                                            <button
                                                class="px-3 py-1 text-xs rounded bg-rose-600 text-white font-bold hover:bg-rose-700 shadow-xs cursor-pointer"
                                                on:click=move |_| handle_empty_trash()
                                            >
                                                "确认清空"
                                            </button>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}

                        // Main Content: List of trash items
                        <div class="flex-1 overflow-y-auto p-6 space-y-3">
                            {move || {
                                if is_loading.get() {
                                    return view! {
                                        <div class="p-12 text-center text-xs text-slate-400">
                                            <span class="inline-block animate-spin mr-1">"↻"</span>
                                            "加载回收站项目..."
                                        </div>
                                    }.into_any();
                                }

                                let list = get_filtered_items();

                                if list.is_empty() {
                                    return view! {
                                        <div class="p-16 text-center text-xs text-slate-400">
                                            <p class="text-2xl mb-2">"🍃"</p>
                                            <p class="text-sm font-semibold text-slate-600 dark:text-slate-400">"回收站空空如也"</p>
                                            <p class="mt-1 text-slate-400">"被删除的卷目、章节与设定将暂存于此，随时可以找回。"</p>
                                        </div>
                                    }.into_any();
                                }

                                view! {
                                    <div class="space-y-3">
                                        {list.into_iter().map(|item| {
                                            let (badge_text, badge_class) = match item.item_type.as_str() {
                                                "volume" => ("卷目", "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300 border-amber-300 dark:border-amber-800"),
                                                "chapter" => ("章节", "bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300 border-blue-300 dark:border-blue-800"),
                                                "character" => ("角色", "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300 border-emerald-300 dark:border-emerald-800"),
                                                "plot" => ("剧情", "bg-purple-100 text-purple-700 dark:bg-purple-950 dark:text-purple-300 border-purple-300 dark:border-purple-800"),
                                                _ => ("世界观", "bg-cyan-100 text-cyan-700 dark:bg-cyan-950 dark:text-cyan-300 border-cyan-300 dark:border-cyan-800"),
                                            };

                                            let type_for_restore = item.item_type.clone();
                                            let id_for_restore = item.id.clone();
                                            let type_for_purge = item.item_type.clone();
                                            let id_for_purge = item.id.clone();
                                            let item_title = item.title;
                                            let item_snippet = item.snippet;
                                            let item_deleted_at = item.deleted_at;

                                            let id_for_op1 = item.id.clone();
                                            let id_for_op2 = item.id.clone();

                                            view! {
                                                <div class="p-3.5 rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-850 flex items-center justify-between gap-4 shadow-xs hover:border-slate-300 dark:hover:border-slate-700 transition-colors">
                                                    <div class="flex items-center gap-3 flex-1 min-w-0">
                                                        <span class=format!("text-xs px-2 py-0.5 rounded border font-semibold shrink-0 {}", badge_class)>
                                                            {badge_text}
                                                        </span>

                                                        <div class="flex-1 min-w-0">
                                                            <div class="flex items-center gap-2">
                                                                <h4 class="text-sm font-bold text-slate-800 dark:text-slate-100 truncate">
                                                                    {item_title}
                                                                </h4>
                                                                <span class="text-xs text-slate-400 font-mono shrink-0">
                                                                    "删除于 " {item_deleted_at}
                                                                </span>
                                                            </div>
                                                            {if !item_snippet.is_empty() {
                                                                view! {
                                                                    <p class="text-xs text-slate-500 dark:text-slate-400 truncate mt-0.5 font-serif">
                                                                        {item_snippet}
                                                                    </p>
                                                                }.into_any()
                                                            } else {
                                                                view! { <span></span> }.into_any()
                                                            }}
                                                        </div>
                                                    </div>

                                                    <div class="flex items-center gap-2 shrink-0">
                                                        <button
                                                            class="px-3 py-1 rounded-lg text-xs font-semibold bg-blue-50 hover:bg-blue-100 dark:bg-blue-950/40 dark:hover:bg-blue-900/60 text-blue-700 dark:text-blue-300 border border-blue-300 dark:border-blue-700/60 shadow-xs flex items-center gap-1 transition-colors cursor-pointer disabled:opacity-50"
                                                            disabled=move || operating_id.get().as_deref() == Some(&id_for_op1)
                                                            on:click=move |_| handle_restore(type_for_restore.clone(), id_for_restore.clone())
                                                        >
                                                            <span>"⎌"</span>
                                                            "恢复"
                                                        </button>

                                                        <button
                                                            class="px-2.5 py-1 rounded-lg text-xs font-semibold bg-rose-50 hover:bg-rose-100 dark:bg-rose-950/40 dark:hover:bg-rose-900/60 text-rose-600 dark:text-rose-400 border border-rose-300 dark:border-rose-800/60 transition-colors cursor-pointer disabled:opacity-50"
                                                            disabled=move || operating_id.get().as_deref() == Some(&id_for_op2)
                                                            title="物理彻底抹除"
                                                            on:click=move |_| handle_purge(type_for_purge.clone(), id_for_purge.clone())
                                                        >
                                                            <span>"✕"</span>
                                                            "彻底删除"
                                                        </button>
                                                    </div>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}

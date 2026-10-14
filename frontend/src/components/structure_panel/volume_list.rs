use crate::components::structure_panel::chapter_list::{
    create_chapter_in_volume, delete_chapter_by_id, request_switch_chapter,
};
use crate::state::AppContext;
use crate::tauri_api::invoke;
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{ChapterHeader, Volume};
use std::collections::HashSet;

#[derive(Serialize)]
struct CreateVolumeArgs {
    novel_id: String,
    title: String,
    summary: String,
}

#[derive(Serialize)]
struct DeleteVolumeArgs {
    id: String,
}

const ITEM_HEIGHT: f64 = 34.0;

#[derive(Clone, PartialEq)]
enum TreeNode {
    VolumeHeader {
        id: String,
        title: String,
        is_collapsed: bool,
        matching_count: usize,
        total_count: usize,
        is_filtering: bool,
    },
    ChapterRow {
        header: ChapterHeader,
    },
    AddChapterBtn {
        volume_id: String,
    },
}

impl TreeNode {
    fn key(&self) -> String {
        match self {
            TreeNode::VolumeHeader { id, .. } => format!("vol_{}", id),
            TreeNode::ChapterRow { header } => format!("ch_{}", header.id),
            TreeNode::AddChapterBtn { volume_id } => format!("add_{}", volume_id),
        }
    }
}

#[component]
pub fn VolumeList(app_ctx: AppContext) -> impl IntoView {
    let collapsed_volumes = RwSignal::new(HashSet::<String>::new());
    let container_ref = NodeRef::<leptos::html::Div>::new();
    let scroll_top = RwSignal::new(0.0f64);
    let container_height = RwSignal::new(600.0f64);

    let on_create_volume = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke::<Volume, _>(
                    "create_volume",
                    &CreateVolumeArgs {
                        novel_id,
                        title: "新卷".to_string(),
                        summary: "".to_string(),
                    },
                )
                .await;

                if let Ok(new_vol) = res {
                    let mut list = app_ctx.volumes_list.get();
                    list.push(new_vol);
                    app_ctx.volumes_list.set(list);
                }
            });
        }
    };

    let on_delete_volume = move |v_id: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("delete_volume", &DeleteVolumeArgs { id: v_id.clone() }).await;
            let mut list = app_ctx.volumes_list.get();
            list.retain(|v| v.id != v_id);
            app_ctx.volumes_list.set(list);
        });
    };

    // Calculate flattened tree nodes reactively
    let flat_tree_nodes = move || {
        let volumes = app_ctx.volumes_list.get();
        let all_chapters = app_ctx.chapters_map.get();
        let query = app_ctx.chapter_search_query.get().to_lowercase().trim().to_string();
        let status_filter = app_ctx.chapter_status_filter.get();
        let is_filtering = !query.is_empty() || status_filter != "all";
        let collapsed = collapsed_volumes.get();

        let mut nodes = Vec::new();
        let mut has_match = false;

        for vol in &volumes {
            let mut matching_chapters = Vec::new();
            let mut total_vol_chapters = 0;

            for ch in &all_chapters {
                if ch.volume_id == vol.id {
                    total_vol_chapters += 1;
                    let match_q = query.is_empty() || ch.title.to_lowercase().contains(&query);
                    let match_s = status_filter == "all" || ch.status == status_filter;
                    if match_q && match_s {
                        matching_chapters.push(ch.clone());
                    }
                }
            }

            if is_filtering && matching_chapters.is_empty() {
                continue;
            }
            if !matching_chapters.is_empty() {
                has_match = true;
            }

            let is_collapsed = if is_filtering {
                false
            } else {
                collapsed.contains(&vol.id)
            };

            nodes.push(TreeNode::VolumeHeader {
                id: vol.id.clone(),
                title: vol.title.clone(),
                is_collapsed,
                matching_count: matching_chapters.len(),
                total_count: total_vol_chapters,
                is_filtering,
            });

            if !is_collapsed {
                for ch in matching_chapters {
                    nodes.push(TreeNode::ChapterRow { header: ch });
                }
                if !is_filtering {
                    nodes.push(TreeNode::AddChapterBtn {
                        volume_id: vol.id.clone(),
                    });
                }
            }
        }

        (nodes, volumes.is_empty(), is_filtering && !has_match)
    };

    // Visible virtual slice
    let visible_nodes = move || {
        let (nodes, _, _) = flat_tree_nodes();
        let total = nodes.len();
        if total <= 30 {
            return nodes;
        }
        let st = scroll_top.get();
        let ch = container_height.get();
        let buffer = 8;
        let start_idx = ((st / ITEM_HEIGHT) as usize).saturating_sub(buffer);
        let visible_count = ((ch / ITEM_HEIGHT) as usize) + buffer * 2;
        let end_idx = (start_idx + visible_count).min(total);
        nodes[start_idx..end_idx].to_vec()
    };

    let spacer_heights = move || {
        let (nodes, _, _) = flat_tree_nodes();
        let total = nodes.len();
        if total <= 30 {
            return (0.0f64, 0.0f64);
        }
        let st = scroll_top.get();
        let ch = container_height.get();
        let buffer = 8;
        let start_idx = ((st / ITEM_HEIGHT) as usize).saturating_sub(buffer);
        let visible_count = ((ch / ITEM_HEIGHT) as usize) + buffer * 2;
        let end_idx = (start_idx + visible_count).min(total);
        let top = start_idx as f64 * ITEM_HEIGHT;
        let bottom = (total - end_idx) as f64 * ITEM_HEIGHT;
        (top, bottom)
    };

    // Auto-scroll to active chapter when chapter changes
    Effect::new(move |_| {
        if let Some(curr) = app_ctx.current_chapter.get() {
            let cid = curr.id;
            let (nodes, _, _) = flat_tree_nodes();
            if let Some(idx) = nodes.iter().position(|n| matches!(n, TreeNode::ChapterRow { header } if header.id == cid)) {
                if let Some(el) = container_ref.get() {
                    let target_top = idx as f64 * ITEM_HEIGHT;
                    let current_top = el.scroll_top() as f64;
                    let current_bottom = current_top + el.client_height() as f64;
                    if current_bottom > current_top && (target_top < current_top || target_top + ITEM_HEIGHT > current_bottom) {
                        el.set_scroll_top((target_top - 60.0).max(0.0) as i32);
                    }
                }
            }
        }
    });

    view! {
        <div class="flex-1 min-h-0 flex flex-col select-none">
            // Header Bar (fixed)
            <div class="flex items-center justify-between px-3 py-1.5 border-b border-slate-200/60 dark:border-slate-800/60 shrink-0 bg-slate-50/80 dark:bg-slate-900/80">
                <div class="flex items-center gap-1.5">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"卷目与章节"</span>
                    {move || {
                        let total_ch = app_ctx.chapters_map.get().len();
                        if total_ch > 0 {
                            view! {
                                <span class="text-[10px] px-1.5 py-0.2 rounded-full font-mono bg-slate-200 dark:bg-slate-800 text-slate-600 dark:text-slate-400">
                                    {format!("{} 章", total_ch)}
                                </span>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }
                    }}
                </div>
                <div class="flex items-center gap-1.5">
                    <button
                        class="text-[11px] text-slate-500 dark:text-slate-400 hover:text-blue-600 dark:hover:text-blue-400 font-medium px-1.5 py-0.5 rounded hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors cursor-pointer"
                        title="批量导入外部 txt/md 文稿并智能拆卷拆章"
                        on:click=move |_| app_ctx.is_import_modal_open.set(true)
                    >
                        "📥 导入"
                    </button>
                    <button
                        class="text-xs text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-300 font-medium px-2 py-0.5 rounded hover:bg-blue-50 dark:hover:bg-blue-950/40 transition-colors cursor-pointer"
                        on:click=on_create_volume
                    >
                        "+ 新建卷"
                    </button>
                </div>
            </div>

            // Scrollable Virtual Viewport
            <div
                node_ref=container_ref
                class="flex-1 min-h-0 overflow-y-auto px-2 py-1 relative"
                on:scroll=move |_| {
                    if let Some(el) = container_ref.get() {
                        scroll_top.set(el.scroll_top() as f64);
                        let h = el.client_height() as f64;
                        if h > 0.0 {
                            container_height.set(h);
                        }
                    }
                }
            >
                {move || {
                    let (_, is_empty, no_match) = flat_tree_nodes();
                    if is_empty {
                        return view! {
                            <div class="py-12 text-center text-xs text-slate-400">
                                "暂无卷目，点击上方「+ 新建卷」或「📥 导入」"
                            </div>
                        }.into_any();
                    }
                    if no_match {
                        return view! {
                            <div class="py-12 text-center text-xs text-slate-400">
                                "未找到匹配的章节"
                            </div>
                        }.into_any();
                    }

                    view! {
                        <div>
                            // Top Virtual Spacer
                            <div
                                style=move || format!("height: {:.0}px", spacer_heights().0)
                                aria-hidden="true"
                            ></div>

                            // Rendered Windowed Nodes
                            <For
                                each=visible_nodes
                                key=|node| node.key()
                                children=move |node| {
                                    match node {
                                        TreeNode::VolumeHeader {
                                            id,
                                            title,
                                            is_collapsed,
                                            matching_count,
                                            total_count,
                                            is_filtering,
                                        } => {
                                            let vid = id.clone();
                                            let del_id = id.clone();
                                            view! {
                                                <div
                                                    style=format!("height: {:.0}px", ITEM_HEIGHT)
                                                    class="box-border flex items-center justify-between px-2.5 my-0.5 rounded-lg bg-slate-100/90 dark:bg-slate-800/90 hover:bg-slate-200/80 dark:hover:bg-slate-750 cursor-pointer text-xs font-semibold text-slate-800 dark:text-slate-100 border border-slate-200/80 dark:border-slate-700/80 select-none group transition-colors shadow-2xs"
                                                    on:click=move |_| {
                                                        if !is_filtering {
                                                            let v = vid.clone();
                                                            collapsed_volumes.update(|set| {
                                                                if set.contains(&v) {
                                                                    set.remove(&v);
                                                                } else {
                                                                    set.insert(v);
                                                                }
                                                            });
                                                        }
                                                    }
                                                >
                                                    <div class="flex items-center gap-1.5 truncate flex-1 min-w-0">
                                                        <span class="text-slate-400 text-[10px] w-3 text-center">
                                                            {if is_collapsed { "▸" } else { "▾" }}
                                                        </span>
                                                        <span class="truncate">{title}</span>
                                                        <span class="text-[10px] text-slate-400 dark:text-slate-500 font-normal">
                                                            {if is_filtering {
                                                                format!("({}/{})", matching_count, total_count)
                                                            } else {
                                                                format!("({})", total_count)
                                                            }}
                                                        </span>
                                                    </div>

                                                    <div class="flex items-center gap-1 shrink-0">
                                                        <button
                                                            class="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-1 rounded hover:bg-slate-200 dark:hover:bg-slate-700 transition-opacity cursor-pointer"
                                                            title="删除此卷（移入回收站）"
                                                            on:click=move |ev| {
                                                                ev.stop_propagation();
                                                                on_delete_volume(del_id.clone());
                                                            }
                                                        >
                                                            "✕"
                                                        </button>
                                                    </div>
                                                </div>
                                            }.into_any()
                                        }

                                        TreeNode::ChapterRow { header } => {
                                            let ch_id = header.id.clone();
                                            let del_cid = header.id.clone();
                                            let ch_clone = header.clone();
                                            let is_active = move || {
                                                app_ctx
                                                    .current_chapter
                                                    .get()
                                                    .map(|c| c.id == ch_id)
                                                    .unwrap_or(false)
                                            };

                                            view! {
                                                <div
                                                    style=format!("height: {:.0}px", ITEM_HEIGHT)
                                                    class=move || {
                                                        format!(
                                                            "box-border group flex items-center justify-between pl-5 pr-2 my-0.5 rounded-md text-xs cursor-pointer transition-colors {}",
                                                            if is_active() {
                                                                "bg-blue-50 text-blue-700 font-semibold border-l-2 border-blue-600 dark:bg-blue-950/60 dark:text-blue-300 dark:border-blue-400"
                                                            } else {
                                                                "text-slate-700 hover:bg-slate-100 hover:text-slate-900 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-slate-100"
                                                            }
                                                        )
                                                    }
                                                    on:click=move |_| request_switch_chapter(app_ctx, ch_clone.clone())
                                                >
                                                    <div class="flex items-center gap-1.5 truncate flex-1 min-w-0 pr-1">
                                                        <span class="text-slate-400 group-hover:text-slate-600 dark:group-hover:text-slate-300 text-[11px] shrink-0">"📄"</span>
                                                        <span class="truncate">{header.title.clone()}</span>
                                                    </div>

                                                    <div class="flex items-center gap-1.5 shrink-0 text-[10px]">
                                                        {match header.status.as_str() {
                                                            "finalized" => view! {
                                                                <span class="px-1 py-0.2 rounded font-medium bg-emerald-100 dark:bg-emerald-950/80 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
                                                                    "定稿"
                                                                </span>
                                                            }.into_any(),
                                                            "editing" => view! {
                                                                <span class="px-1 py-0.2 rounded font-medium bg-blue-100 dark:bg-blue-950/80 text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800">
                                                                    "修改"
                                                                </span>
                                                            }.into_any(),
                                                            _ => view! {
                                                                <span class="px-1 py-0.2 rounded font-medium bg-amber-100 dark:bg-amber-950/80 text-amber-700 dark:text-amber-300 border border-amber-200 dark:border-amber-800">
                                                                    "草稿"
                                                                </span>
                                                            }.into_any(),
                                                        }}
                                                        <span class="text-slate-400 dark:text-slate-500 font-normal">{header.word_count} "字"</span>
                                                        <button
                                                            class="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-rose-500 dark:hover:text-rose-400 text-xs p-0.5 rounded transition-opacity cursor-pointer shrink-0"
                                                            title="删除章节（移入回收站）"
                                                            on:click=move |ev| {
                                                                ev.stop_propagation();
                                                                delete_chapter_by_id(app_ctx, del_cid.clone());
                                                            }
                                                        >
                                                            "✕"
                                                        </button>
                                                    </div>
                                                </div>
                                            }.into_any()
                                        }

                                        TreeNode::AddChapterBtn { volume_id } => {
                                            let vid = volume_id.clone();
                                            view! {
                                                <button
                                                    style=format!("height: {:.0}px", ITEM_HEIGHT)
                                                    class="box-border w-full text-left pl-5 pr-2 my-0.5 text-[11px] text-slate-400 hover:text-blue-600 hover:bg-slate-100 dark:hover:bg-slate-800 dark:hover:text-blue-400 rounded-md flex items-center gap-1 transition-colors cursor-pointer"
                                                    on:click=move |_| create_chapter_in_volume(app_ctx, vid.clone())
                                                >
                                                    <span>"+"</span>
                                                    <span>"添加新章节"</span>
                                                </button>
                                            }.into_any()
                                        }
                                    }
                                }
                            />

                            // Bottom Virtual Spacer
                            <div
                                style=move || format!("height: {:.0}px", spacer_heights().1)
                                aria-hidden="true"
                            ></div>
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

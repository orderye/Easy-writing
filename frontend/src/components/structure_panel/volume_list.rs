use crate::components::structure_panel::chapter_list::{
    create_chapter_in_volume, delete_chapter_by_id, request_switch_chapter,
};
use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;
use shared_types::{ChapterHeader, Volume};
use std::collections::HashSet;
use wasm_bindgen::JsCast;

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

    // ---- 拖拽排序状态 ----
    // 拖拽中的对象：章节（记录所属卷，用于判断是同卷排序还是跨卷移动）或卷。
    // 用 (类型, id, 所属卷 id) 三元组描述，None 表示当前无拖拽。
    let dragging_chapter = RwSignal::new(Option::<(String, String)>::None); // (chapter_id, volume_id)
    let dragging_volume = RwSignal::new(Option::<String>::None);
    // 悬停目标，用于绘制插入位置指示线
    let drop_target = RwSignal::new(Option::<(String, bool)>::None); // (id, is_before)

    let on_create_volume = move |_| {
        if let Some(novel) = app_ctx.current_novel.get() {
            let novel_id = novel.id;
            leptos::task::spawn_local(async move {
                let res = invoke_cmd("create_volume")
                    .arg("novel_id", novel_id)
                    .arg("title", "新卷".to_string())
                    .arg("summary", "".to_string())
                    .send::<Volume>()
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
        // 若当前正在编辑的章节属于被删卷且未保存，阻断并提醒
        if let Some(curr) = app_ctx.current_chapter.get() {
            if curr.volume_id == v_id && app_ctx.save_status.get() != "saved" {
                app_ctx.save_alert.set(Some(
                    "该卷包含当前未保存的章节，请先保存或放弃修改后再删除。".to_string(),
                ));
                return;
            }
        }

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("delete_volume")
                .arg("id", v_id.clone())
                .send_unit()
                .await;
            match res {
                Ok(()) => {
                    let mut list = app_ctx.volumes_list.get();
                    list.retain(|v| v.id != v_id);
                    app_ctx.volumes_list.set(list.clone());

                    // 同步清理属于该卷的章节
                    let mut ch_list = app_ctx.chapters_map.get();
                    ch_list.retain(|c| c.volume_id != v_id);
                    app_ctx.chapters_map.set(ch_list.clone());

                    if let Some(curr) = app_ctx.current_chapter.get() {
                        if curr.volume_id == v_id {
                            if let Some(next_ch) = ch_list.first() {
                                crate::components::structure_panel::chapter_list::do_switch_chapter(app_ctx, next_ch.id.clone());
                            } else {
                                app_ctx.current_chapter.set(None);
                                crate::bridge::tiptap::set_content("");
                                crate::bridge::tiptap::set_editable(false);
                                app_ctx.word_count.set(0);
                                app_ctx.save_status.set("saved".to_string());
                            }
                        }
                    }

                    if let Some(curr_v) = app_ctx.current_volume.get() {
                        if curr_v.id == v_id {
                            app_ctx.current_volume.set(list.first().cloned());
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("删除卷失败: {}", e).into());
                    app_ctx.save_alert.set(Some(format!("删除卷失败：{}", e)));
                }
            }
        });
    };


    // 把拖拽结果落库：同卷内重排 → reorder_chapters；跨卷 → move_chapter。
    //
    let reload_chapters_from_db = move || {
        leptos::task::spawn_local(async move {
            let volumes = app_ctx.volumes_list.get_untracked();
            let mut all_chs = Vec::new();
            for v in volumes {
                if let Ok(chs) = invoke_cmd("list_chapter_headers")
                    .arg("volume_id", v.id)
                    .send::<Vec<ChapterHeader>>()
                    .await
                {
                    all_chs.extend(chs);
                }
            }
            app_ctx.chapters_map.set(all_chs);
        });
    };

    let reload_volumes_from_db = move || {
        leptos::task::spawn_local(async move {
            if let Some(novel) = app_ctx.current_novel.get_untracked() {
                if let Ok(vols) = invoke_cmd("list_volumes")
                    .arg("novel_id", novel.id)
                    .send::<Vec<Volume>>()
                    .await
                {
                    app_ctx.volumes_list.set(vols);
                }
            }
        });
    };

    // 把拖拽结果落库：同卷内重排 → reorder_chapters；跨卷 → move_chapter。
    //
    // 采用乐观更新提供即时反馈，若落库失败则立刻回滚并从数据库重新对齐，绝不残留脏状态。
    let apply_chapter_drop = move |dragged_id: String,
                                   from_volume: String,
                                   target_id: String,
                                   is_before: bool| {
        let old_chapters = app_ctx.chapters_map.get();
        // 以当前展示顺序为基准计算新的 id 序列
        let mut list = old_chapters.clone();
        let Some(from_idx) = list.iter().position(|c| c.id == dragged_id) else {
            return;
        };
        let Some(to_idx) = list.iter().position(|c| c.id == target_id) else {
            return;
        };
        if dragged_id == target_id {
            return;
        }

        let target_volume = list[to_idx].volume_id.clone();
        let same_volume = from_volume == target_volume;

        // 取出被拖拽项
        let moved = list.remove(from_idx);
        // 移除后重新定位目标（若被拖项在目标之前，目标索引会前移一位）
        let mut target_idx = list
            .iter()
            .position(|c| c.id == target_id)
            .unwrap_or(to_idx.min(list.len()));
        if !is_before {
            target_idx += 1;
        }
        target_idx = target_idx.min(list.len());

        let rollback_list = old_chapters;
        if same_volume {
            // 同卷排序：插入到目标位置后重写该卷的 sort_order 序列
            list.insert(target_idx, moved.clone());
            app_ctx.chapters_map.set(list.clone());

            let ordered_ids: Vec<String> = list
                .iter()
                .filter(|c| c.volume_id == target_volume)
                .map(|c| c.id.clone())
                .collect();

            leptos::task::spawn_local(async move {
                let res = invoke_cmd("reorder_chapters")
                    .arg("chapter_ids", ordered_ids)
                    .send_unit()
                    .await;
                if let Err(e) = res {
                    web_sys::console::error_1(&format!("章节排序失败: {}", e).into());
                    app_ctx.chapters_map.set(rollback_list);
                    reload_chapters_from_db();
                    app_ctx.save_alert.set(Some(format!("章节排序保存失败，已恢复原顺序：{}", e)));
                }
            });
        } else {
            // 跨卷移动：改 volume_id 并给出卷内新序号
            let mut moved_final = moved.clone();
            moved_final.volume_id = target_volume.clone();
            let new_sort_order = target_idx as i32;
            list.insert(target_idx, moved_final);
            app_ctx.chapters_map.set(list);

            leptos::task::spawn_local(async move {
                let res = invoke_cmd("move_chapter")
                    .arg("chapter_id", dragged_id)
                    .arg("new_volume_id", target_volume)
                    .arg("new_sort_order", new_sort_order)
                    .send_unit()
                    .await;
                if let Err(e) = res {
                    web_sys::console::error_1(&format!("跨卷移动章节失败: {}", e).into());
                    app_ctx.chapters_map.set(rollback_list);
                    reload_chapters_from_db();
                    app_ctx.save_alert.set(Some(format!("跨卷移动章节保存失败，已恢复原位置：{}", e)));
                }
            });
        }
    };

    // 卷拖拽排序：按新顺序重写 volume.sort_order。
    let apply_volume_drop = move |dragged_id: String, target_id: String, is_before: bool| {
        if dragged_id == target_id {
            return;
        }
        let old_volumes = app_ctx.volumes_list.get();
        let mut list = old_volumes.clone();
        let Some(from_idx) = list.iter().position(|v| v.id == dragged_id) else {
            return;
        };
        let moved = list.remove(from_idx);
        let mut target_idx = list
            .iter()
            .position(|v| v.id == target_id)
            .unwrap_or(0);
        if !is_before {
            target_idx += 1;
        }
        target_idx = target_idx.min(list.len());
        list.insert(target_idx, moved);
        app_ctx.volumes_list.set(list.clone());

        let rollback_volumes = old_volumes;
        let ordered_ids: Vec<String> = list.iter().map(|v| v.id.clone()).collect();
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("reorder_volumes")
                .arg("volume_ids", ordered_ids)
                .send_unit()
                .await;
            if let Err(e) = res {
                web_sys::console::error_1(&format!("卷目排序失败: {}", e).into());
                app_ctx.volumes_list.set(rollback_volumes);
                reload_volumes_from_db();
                app_ctx.save_alert.set(Some(format!("分卷排序保存失败，已恢复原顺序：{}", e)));
            }
        });
    };


    // Calculate flattened tree nodes reactively
    let flat_tree_nodes = move || {
        let volumes = app_ctx.volumes_list.get();
        let all_chapters = app_ctx.chapters_map.get();
        let query = app_ctx
            .chapter_search_query
            .get()
            .to_lowercase()
            .trim()
            .to_string();
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
            if let Some(idx) = nodes
                .iter()
                .position(|n| matches!(n, TreeNode::ChapterRow { header } if header.id == cid))
            {
                if let Some(el) = container_ref.get() {
                    let target_top = idx as f64 * ITEM_HEIGHT;
                    let current_top = el.scroll_top() as f64;
                    let current_bottom = current_top + el.client_height() as f64;
                    if current_bottom > current_top
                        && (target_top < current_top || target_top + ITEM_HEIGHT > current_bottom)
                    {
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
                                            let drag_id = id.clone();
                                            let drop_id = id.clone();
                                            let hover_id = id.clone();
                                            let is_drag_source = {
                                                let v = id.clone();
                                                move || dragging_volume.get().as_deref() == Some(v.as_str())
                                            };
                                            let is_drop_before = {
                                                let v = id.clone();
                                                move || {
                                                    drop_target
                                                        .get()
                                                        .map(|(t, before)| t == v && before)
                                                        .unwrap_or(false)
                                                }
                                            };
                                            view! {
                                                <div
                                                    style=format!("height: {:.0}px", ITEM_HEIGHT)
                                                    class=move || {
                                                        format!(
                                                            "box-border flex items-center justify-between px-2.5 my-0.5 rounded-lg bg-slate-100/90 dark:bg-slate-800/90 hover:bg-slate-200/80 dark:hover:bg-slate-750 cursor-pointer text-xs font-semibold text-slate-800 dark:text-slate-100 border border-slate-200/80 dark:border-slate-700/80 select-none group transition-colors shadow-2xs {}",
                                                            if is_drag_source() {
                                                                "opacity-40"
                                                            } else if is_drop_before() {
                                                                "border-t-2 border-blue-500"
                                                            } else {
                                                                ""
                                                            }
                                                        )
                                                    }
                                                    draggable="true"
                                                    on:dragstart=move |ev| {
                                                        dragging_volume.set(Some(drag_id.clone()));
                                                        dragging_chapter.set(None);
                                                        if let Some(dt) = ev.data_transfer() {
                                                            let _ = dt.set_data("text/plain", "volume");
                                                        }
                                                    }
                                                    on:dragover=move |ev| {
                                                        ev.prevent_default();
                                                        let before = ev
                                                            .current_target()
                                                            .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                                                            .map(|el| {
                                                                let rect = el.get_bounding_client_rect();
                                                                let y = ev.client_y() as f64;
                                                                y < rect.top() + rect.height() / 2.0
                                                            })
                                                            .unwrap_or(true);
                                                        drop_target.set(Some((hover_id.clone(), before)));
                                                    }
                                                    on:drop=move |ev| {
                                                        ev.prevent_default();
                                                        let before = drop_target
                                                            .get()
                                                            .filter(|(t, _)| *t == drop_id)
                                                            .map(|(_, b)| b)
                                                            .unwrap_or(true);
                                                        if let Some(dragged) = dragging_volume.get() {
                                                            apply_volume_drop(dragged, drop_id.clone(), before);
                                                        }
                                                        drop_target.set(None);
                                                    }
                                                    on:dragend=move |_| {
                                                        dragging_volume.set(None);
                                                        drop_target.set(None);
                                                    }
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
                                            let drag_cid = header.id.clone();
                                            let drag_vol = header.volume_id.clone();
                                            let drop_cid = header.id.clone();
                                            let hover_cid = header.id.clone();
                                            let is_active = {
                                                let id = ch_id.clone();
                                                move || {
                                                    app_ctx
                                                        .current_chapter
                                                        .get()
                                                        .map(|c| c.id == id)
                                                        .unwrap_or(false)
                                                }
                                            };
                                            let is_drag_source = {
                                                let id = ch_id.clone();
                                                move || {
                                                    dragging_chapter
                                                        .get()
                                                        .map(|(cid, _)| cid == id)
                                                        .unwrap_or(false)
                                                }
                                            };
                                            let is_drop_before = {
                                                let id = ch_id.clone();
                                                move || {
                                                    drop_target
                                                        .get()
                                                        .map(|(t, before)| t == id && before)
                                                        .unwrap_or(false)
                                                }
                                            };

                                            view! {
                                                <div
                                                    style=format!("height: {:.0}px", ITEM_HEIGHT)
                                                    class=move || {
                                                        format!(
                                                            "box-border group flex items-center justify-between pl-5 pr-2 my-0.5 rounded-md text-xs cursor-pointer transition-colors {} {}",
                                                            if is_active() {
                                                                "bg-blue-50 text-blue-700 font-semibold border-l-2 border-blue-600 dark:bg-blue-950/60 dark:text-blue-300 dark:border-blue-400"
                                                            } else {
                                                                "text-slate-700 hover:bg-slate-100 hover:text-slate-900 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-slate-100"
                                                            },
                                                            // 拖拽反馈：拖动中半透明；作为落点时显示上/下插入线
                                                            if is_drag_source() {
                                                                "opacity-40"
                                                            } else if is_drop_before() {
                                                                "border-t-2 border-blue-500"
                                                            } else {
                                                                ""
                                                            }
                                                        )
                                                    }
                                                    draggable="true"
                                                    on:dragstart=move |ev| {
                                                        dragging_chapter
                                                            .set(Some((drag_cid.clone(), drag_vol.clone())));
                                                        dragging_volume.set(None);
                                                        if let Some(dt) = ev.data_transfer() {
                                                            let _ = dt.set_data("text/plain", "chapter");
                                                        }
                                                        // 阻止冒泡到卷的 dragstart，避免卷被同时拖动
                                                        ev.stop_propagation();
                                                    }
                                                    on:dragover=move |ev| {
                                                        ev.prevent_default();
                                                        ev.stop_propagation();
                                                        let before = ev
                                                            .current_target()
                                                            .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                                                            .map(|el| {
                                                                let rect = el.get_bounding_client_rect();
                                                                let y = ev.client_y() as f64;
                                                                y < rect.top() + rect.height() / 2.0
                                                            })
                                                            .unwrap_or(true);
                                                        drop_target.set(Some((hover_cid.clone(), before)));
                                                    }
                                                    on:drop=move |ev| {
                                                        ev.prevent_default();
                                                        ev.stop_propagation();
                                                        let before = drop_target
                                                            .get()
                                                            .filter(|(t, _)| *t == drop_cid)
                                                            .map(|(_, b)| b)
                                                            .unwrap_or(true);
                                                        if let Some((dragged, from_vol)) = dragging_chapter.get() {
                                                            apply_chapter_drop(
                                                                dragged,
                                                                from_vol,
                                                                drop_cid.clone(),
                                                                before,
                                                            );
                                                        }
                                                        drop_target.set(None);
                                                    }
                                                    on:dragend=move |_| {
                                                        dragging_chapter.set(None);
                                                        drop_target.set(None);
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

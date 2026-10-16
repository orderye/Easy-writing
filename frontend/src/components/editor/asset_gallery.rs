use crate::components::editor::image_upload::{pick_and_save_image, UploadEvent};
use crate::state::AppContext;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use leptos::prelude::*;
use shared_types::{AssetInfo, ImageAssetUsage, ImageScanReport};
use std::collections::HashSet;

/// 字节数 → 人类可读体积。
fn format_size(bytes: i64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn no_usage() -> ImageAssetUsage {
    ImageAssetUsage {
        referencing_chapters: 0,
        referencing_revisions: 0,
        shared_records: 0,
    }
}

/// 图库：本作品图片资产的浏览、复用、体检与维护。
///
/// 【为什么值得单独做一个界面】
/// 图片一旦入库就再没有被复用的入口——用户在别的章节想用同一张图，只能重新
/// 上传一遍（磁盘不会重复，但库里会多一条记录）。图库解决四件事：
///   1. 本作品有哪些图（列表 + 缩略图 + 统计）；
///   2. 它们还在不在（渲染失败 → 「文件缺失」徽标，把 WebDAV 恢复补图失败
///      从「正文里默默裂图」变成「图库一眼可见」）；
///   3. 怎么把已入库的图插回正文（不必重新上传）；
///   4. 命名与移除（软删除可恢复；磁盘文件不由这里删，见下）。
///
/// 【为什么移除不删文件】
/// 正文与历史版本里存的是 asset URL，删文件会当场让正文裂图；而且同一内容
/// 可能对应多条记录（内容哈希去重）。因此移除只软删记录，文件回收交给
/// 「维护」里的孤儿清理——那里会先确认没有任何记录与正文引用。
#[component]
pub fn AssetGalleryModal(app_ctx: AppContext) -> impl IntoView {
    let is_open = app_ctx.is_asset_gallery_open;
    let assets = RwSignal::new(Vec::<AssetInfo>::new());
    let deleted = RwSignal::new(Vec::<AssetInfo>::new());
    let app_dir = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let sort_mode = RwSignal::new("newest".to_string());
    let selected_id = RwSignal::new(Option::<String>::None);
    let broken = RwSignal::new(HashSet::<String>::new());
    let status = RwSignal::new(String::new());
    let is_loading = RwSignal::new(false);
    let is_uploading = RwSignal::new(false);
    let show_deleted = RwSignal::new(false);
    // 选中项的引用规模（正文 / 历史版本 / 同文件的其他记录）
    let usage = RwSignal::new(no_usage());
    let rename_text = RwSignal::new(String::new());
    let pending_remove = RwSignal::new(Option::<AssetInfo>::None);
    let pending_purge = RwSignal::new(Option::<AssetInfo>::None);
    // 维护：全库范围的图片存储对账
    let scan_report = RwSignal::new(Option::<ImageScanReport>::None);
    let is_scanning = RwSignal::new(false);
    let show_maintenance = RwSignal::new(false);

    // 重新拉取「在库 + 已移除」两份列表与 APPDATA 前缀
    let load = move || {
        let Some(novel) = app_ctx.current_novel.get() else {
            assets.set(Vec::new());
            deleted.set(Vec::new());
            return;
        };
        let novel_id = novel.id;
        is_loading.set(true);
        leptos::task::spawn_local(async move {
            let dir = invoke_no_args::<String>("get_app_data_dir")
                .await
                .unwrap_or_default();
            app_dir.set(dir);

            let novel_for_deleted = novel_id.clone();
            let live = invoke_cmd("list_image_assets")
                .arg("novel_id", novel_id)
                .send::<Vec<AssetInfo>>()
                .await;
            let removed = invoke_cmd("list_deleted_image_assets")
                .arg("novel_id", novel_for_deleted)
                .send::<Vec<AssetInfo>>()
                .await;

            match live {
                Ok(list) => {
                    assets.set(list);
                    status.set(String::new());
                }
                Err(e) => status.set(format!("✕ 读取图库失败：{}", e)),
            }
            if let Ok(list) = removed {
                deleted.set(list);
            }
            is_loading.set(false);
        });
    };

    // 打开时载入；打开状态下切换作品也会重新载入（不新造一套刷新约定）
    Effect::new(move |_| {
        if is_open.get() {
            let _novel = app_ctx.current_novel.get();
            broken.set(HashSet::new());
            selected_id.set(None);
            show_deleted.set(false);
            show_maintenance.set(false);
            status.set(String::new());
            load();
        }
    });

    // 选中项变化时载入引用规模，并把名称填进重命名输入框
    Effect::new(move |_| {
        let Some(target) = selected_id
            .get()
            .and_then(|id| assets.get().into_iter().find(|x| x.id == id))
        else {
            usage.set(no_usage());
            rename_text.set(String::new());
            return;
        };
        rename_text.set(target.name.clone());
        let id = target.id.clone();
        leptos::task::spawn_local(async move {
            if let Ok(u) = invoke_cmd("get_image_asset_usage")
                .arg("asset_id", id)
                .send::<ImageAssetUsage>()
                .await
            {
                usage.set(u);
            }
        });
    });

    // 过滤 + 排序后的可见列表
    let visible = move || {
        let q = query.get().trim().to_lowercase();
        let mut list: Vec<AssetInfo> = assets
            .get()
            .into_iter()
            .filter(|a| q.is_empty() || a.name.to_lowercase().contains(&q))
            .collect();
        match sort_mode.get().as_str() {
            "name" => list.sort_by(|a, b| a.name.cmp(&b.name)),
            "size" => list.sort_by_key(|a| std::cmp::Reverse(a.size)),
            // newest：后端已按 created_at DESC 返回，保持原序
            _ => {}
        }
        list
    };

    let total_bytes = move || assets.get().iter().map(|a| a.size).sum::<i64>();
    let broken_count = move || broken.get().len();

    // 插入到光标处
    let on_insert = move |a: AssetInfo| {
        let dir = app_dir.get();
        if dir.is_empty() {
            status.set("✕ 无法解析图片路径（应用数据目录获取失败），请重试".to_string());
            return;
        }
        let url = crate::bridge::asset::resolve_asset_url(&a.rel_path, &dir);
        crate::bridge::tiptap::insert_image(&url, &a.name);
        app_ctx.save_status.set("dirty".to_string());
        status.set(format!("✓ 已把「{}」插入光标处", a.name));
    };

    // 复制相对路径（便于排查与跨端迁移）
    let on_copy = move |a: AssetInfo| {
        if crate::bridge::asset::copy_asset_path(&a.rel_path) {
            status.set(format!("✓ 已复制路径 {}", a.rel_path));
        } else {
            status.set("✕ 复制失败（剪贴板不可用）".to_string());
        }
    };

    // 重命名（只改显示名，路径与文件不变）
    let do_rename = move || {
        let Some(id) = selected_id.get() else { return };
        let name = rename_text.get().trim().to_string();
        if name.is_empty() {
            status.set("✕ 图片名称不能为空".to_string());
            return;
        }
        leptos::task::spawn_local(async move {
            match invoke_cmd("rename_image_asset")
                .arg("asset_id", id)
                .arg("name", name)
                .send_unit()
                .await
            {
                Ok(_) => {
                    status.set("✓ 已重命名".to_string());
                    load();
                }
                Err(e) => status.set(format!("✕ 重命名失败：{}", e)),
            }
        });
    };

    // 从图库移除（软删除，可恢复；不动磁盘文件）
    let do_remove = move |id: String| {
        leptos::task::spawn_local(async move {
            match invoke_cmd("delete_image_asset")
                .arg("asset_id", id)
                .send_unit()
                .await
            {
                Ok(_) => {
                    status.set("✓ 已从图库移除，可在下方「已移除的图片」中恢复".to_string());
                    selected_id.set(None);
                    load();
                }
                Err(e) => status.set(format!("✕ 移除失败：{}", e)),
            }
        });
    };

    // 恢复
    let do_restore = move |id: String| {
        leptos::task::spawn_local(async move {
            match invoke_cmd("restore_image_asset")
                .arg("asset_id", id)
                .send_unit()
                .await
            {
                Ok(_) => {
                    status.set("✓ 已恢复到图库".to_string());
                    load();
                }
                Err(e) => status.set(format!("✕ 恢复失败：{}", e)),
            }
        });
    };

    // 彻底删除记录（不可恢复）；文件回收不在此处
    let do_purge = move |id: String| {
        leptos::task::spawn_local(async move {
            match invoke_cmd("purge_image_asset")
                .arg("asset_id", id)
                .send_unit()
                .await
            {
                Ok(_) => {
                    status.set("✓ 已彻底删除记录（磁盘文件可在「维护」中清理）".to_string());
                    load();
                }
                Err(e) => status.set(format!("✕ 彻底删除失败：{}", e)),
            }
        });
    };

    // 存储对账（全库作用域）：磁盘孤儿文件 + 记录存在但文件缺失
    let do_scan = move || {
        is_scanning.set(true);
        leptos::task::spawn_local(async move {
            match invoke_no_args::<ImageScanReport>("scan_image_storage").await {
                Ok(report) => {
                    scan_report.set(Some(report));
                    status.set("✓ 已扫描图片存储".to_string());
                }
                Err(e) => status.set(format!("✕ 扫描失败：{}", e)),
            }
            is_scanning.set(false);
        });
    };

    // 只清理「无记录引用且无正文引用」的孤儿文件
    let do_prune = move || {
        is_scanning.set(true);
        leptos::task::spawn_local(async move {
            let pruned = invoke_no_args::<i64>("prune_orphan_images").await;
            let rescanned = invoke_no_args::<ImageScanReport>("scan_image_storage").await;
            match pruned {
                Ok(n) => status.set(format!("✓ 已清理 {} 个无人引用的孤儿文件", n)),
                Err(e) => status.set(format!("✕ 清理失败：{}", e)),
            }
            if let Ok(report) = rescanned {
                scan_report.set(Some(report));
            }
            is_scanning.set(false);
        });
    };

    // 上传（空态按钮）：复用与工具栏同一份上传流程
    let on_upload = move |_| {
        if is_uploading.get() {
            return;
        }
        pick_and_save_image(
            app_ctx,
            move |ev| match ev {
                UploadEvent::Started => {
                    is_uploading.set(true);
                    status.set("正在保存图片...".to_string());
                }
                UploadEvent::Failed(msg) => {
                    status.set(msg);
                    is_uploading.set(false);
                }
            },
            move |_rel_path, name| {
                is_uploading.set(false);
                status.set(format!("✓ 已上传「{}」", name));
                load();
            },
        );
    };

    view! {
        {move || {
            if !is_open.get() {
                return view! { <div></div> }.into_any();
            }
            view! {
                <div class="fixed inset-0 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center z-50 p-6">
                    <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-4xl w-full border border-slate-200 dark:border-slate-700 overflow-hidden flex flex-col max-h-[85vh]">
                        // 标题栏
                        <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-700 flex items-center justify-between bg-slate-50 dark:bg-slate-800/80 shrink-0">
                            <div class="flex items-center gap-2 min-w-0">
                                <span class="text-lg">"🗂"</span>
                                <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100">"图库"</h3>
                                <span class="text-[11px] text-slate-400 truncate">
                                    {move || {
                                        let n = assets.get().len();
                                        format!("本作品 {} 张 · 合计 {}", n, format_size(total_bytes()))
                                    }}
                                </span>
                                {move || if broken_count() > 0 {
                                    view! {
                                        <span class="text-[10px] px-1.5 py-0.5 rounded bg-amber-100 dark:bg-amber-950/50 text-amber-700 dark:text-amber-300 border border-amber-300 dark:border-amber-800 shrink-0">
                                            {format!("⚠ {} 条记录找不到文件", broken_count())}
                                        </span>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}
                            </div>
                            <button
                                class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 font-mono text-lg cursor-pointer shrink-0"
                                on:click=move |_| is_open.set(false)
                            >
                                "✕"
                            </button>
                        </div>

                        // 工具条
                        <div class="px-5 py-2 border-b border-slate-200 dark:border-slate-700 flex items-center gap-2 bg-white dark:bg-slate-900 shrink-0">
                            <div class="relative flex items-center flex-1">
                                <span class="absolute left-2 text-slate-400 text-xs">"🔍"</span>
                                <input
                                    type="text"
                                    class="w-full pl-6 pr-6 py-1 text-xs rounded-md bg-slate-100 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-slate-800 dark:text-slate-100 placeholder-slate-400 focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                    placeholder="按文件名搜索..."
                                    prop:value=move || query.get()
                                    on:input=move |ev| query.set(event_target_value(&ev))
                                />
                            </div>
                            <select
                                class="text-xs py-1 px-1.5 rounded-md bg-slate-100 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 cursor-pointer"
                                on:change=move |ev| sort_mode.set(event_target_value(&ev))
                            >
                                <option value="newest">"最新优先"</option>
                                <option value="name">"按文件名"</option>
                                <option value="size">"按体积"</option>
                            </select>
                            <button
                                class="px-3 py-1 text-xs bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 rounded font-medium transition-colors cursor-pointer disabled:opacity-50 whitespace-nowrap shrink-0"
                                disabled=move || is_uploading.get()
                                on:click=on_upload
                            >
                                {move || if is_uploading.get() { "⏳ 上传中" } else { "＋ 上传图片" }}
                            </button>
                        </div>

                        // 状态提示
                        {move || {
                            let msg = status.get();
                            if msg.is_empty() {
                                return view! { <div></div> }.into_any();
                            }
                            let cls = if msg.starts_with('✓') {
                                "bg-emerald-50 dark:bg-emerald-950/40 border-emerald-200 dark:border-emerald-800 text-emerald-800 dark:text-emerald-300"
                            } else {
                                "bg-amber-50 dark:bg-amber-950/40 border-amber-200 dark:border-amber-800 text-amber-800 dark:text-amber-300"
                            };
                            view! {
                                <div class=format!("px-5 py-1.5 text-[11px] font-medium border-b {}", cls)>
                                    {msg}
                                </div>
                            }.into_any()
                        }}

                        // 主体：网格 + 详情
                        <div class="flex-1 min-h-0 flex flex-row">
                            <div class="flex-1 min-w-0 overflow-y-auto p-4">
                                {move || {
                                    if app_ctx.current_novel.get().is_none() {
                                        return view! {
                                            <div class="h-full flex items-center justify-center text-xs text-slate-400">
                                                "请先在「📚 作品」中选择或新建一个作品"
                                            </div>
                                        }.into_any();
                                    }
                                    if is_loading.get() && assets.get().is_empty() {
                                        return view! {
                                            <div class="h-full flex items-center justify-center text-xs text-slate-400">
                                                "正在载入图库..."
                                            </div>
                                        }.into_any();
                                    }
                                    view! {
                                        <div>
                                            {if assets.get().is_empty() {
                                                view! {
                                                    <div class="flex flex-col items-center justify-center gap-3 py-10 text-xs text-slate-400">
                                                        <span class="text-2xl">"🖼"</span>
                                                        <span>"本作品还没有图片"</span>
                                                        <span class="text-[10px]">"点上方「＋ 上传图片」，或用编辑器工具栏的「🖼 图片」插入"</span>
                                                    </div>
                                                }.into_any()
                                            } else if visible().is_empty() {
                                                view! {
                                                    <div class="flex items-center justify-center py-10 text-xs text-slate-400">
                                                        "没有匹配的图片"
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="grid grid-cols-3 sm:grid-cols-4 gap-2">
                                                        <For
                                                            each=visible
                                                            key=|a| a.id.clone()
                                                            children=move |asset| {
                                                                let a_select = asset.clone();
                                                                let a_class = asset.clone();
                                                                let a_img = asset.clone();
                                                                let a_badge = asset.clone();
                                                                let rel_err = asset.rel_path.clone();
                                                                let id_match = asset.id.clone();
                                                                let name_txt = asset.name.clone();
                                                                let alt_txt = asset.name.clone();
                                                                let size_txt = format_size(asset.size);
                                                                view! {
                                                                    <button
                                                                        class=move || {
                                                                            let active = selected_id
                                                                                .get()
                                                                                .map(|id| id == id_match)
                                                                                .unwrap_or(false);
                                                                            let missing = broken.get().contains(&a_class.rel_path);
                                                                            format!(
                                                                                "group text-left rounded-lg border overflow-hidden transition-colors cursor-pointer {}",
                                                                                if active {
                                                                                    "border-blue-500 ring-1 ring-blue-400"
                                                                                } else if missing {
                                                                                    "border-amber-300 dark:border-amber-800"
                                                                                } else {
                                                                                    "border-slate-200 dark:border-slate-700 hover:border-blue-300"
                                                                                }
                                                                            )
                                                                        }
                                                                        on:click=move |_| selected_id.set(Some(a_select.id.clone()))
                                                                    >
                                                                        <div class="aspect-video bg-slate-100 dark:bg-slate-900 flex items-center justify-center overflow-hidden">
                                                                            <img
                                                                                class="w-full h-full object-cover"
                                                                                loading="lazy"
                                                                                src=move || crate::bridge::asset::resolve_asset_url(
                                                                                    &a_img.rel_path,
                                                                                    &app_dir.get(),
                                                                                )
                                                                                alt=alt_txt
                                                                                on:error=move |_| {
                                                                                    let rel = rel_err.clone();
                                                                                    broken.update(|s| {
                                                                                        s.insert(rel);
                                                                                    });
                                                                                }
                                                                            />
                                                                        </div>
                                                                        <div class="px-1.5 py-1 bg-white dark:bg-slate-800">
                                                                            <div class="text-[10px] text-slate-700 dark:text-slate-200 truncate" title=name_txt.clone()>
                                                                                {name_txt.clone()}
                                                                            </div>
                                                                            <div class="text-[9px] text-slate-400 flex items-center gap-1">
                                                                                <span>{size_txt}</span>
                                                                                {move || if broken.get().contains(&a_badge.rel_path) {
                                                                                    view! { <span class="text-amber-500">"⚠ 缺失"</span> }.into_any()
                                                                                } else {
                                                                                    view! { <span></span> }.into_any()
                                                                                }}
                                                                            </div>
                                                                        </div>
                                                                    </button>
                                                                }
                                                            }
                                                        />
                                                    </div>
                                                }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                }}

                                // 已移除的图片（软删除，可恢复）
                                {move || {
                                    let list = deleted.get();
                                    if list.is_empty() {
                                        return view! { <div></div> }.into_any();
                                    }
                                    view! {
                                        <div class="mt-4 pt-2 border-t border-slate-200 dark:border-slate-700">
                                            <button
                                                class="text-[11px] text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 cursor-pointer"
                                                on:click=move |_| show_deleted.set(!show_deleted.get())
                                            >
                                                {move || format!(
                                                    "{} 已移除的图片（{}）",
                                                    if show_deleted.get() { "▾" } else { "▸" },
                                                    deleted.get().len(),
                                                )}
                                            </button>
                                            {move || if show_deleted.get() {
                                                view! {
                                                    <div class="space-y-1 mt-1.5">
                                                        <For
                                                            each=move || deleted.get()
                                                            key=|a| a.id.clone()
                                                            children=move |a| {
                                                                let a_restore = a.clone();
                                                                let a_purge = a.clone();
                                                                let name = a.name.clone();
                                                                let meta = format!("{} · {}", format_size(a.size), a.rel_path);
                                                                view! {
                                                                    <div class="flex items-center justify-between gap-2 p-1.5 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/60">
                                                                        <div class="min-w-0">
                                                                            <div class="text-[10px] text-slate-600 dark:text-slate-300 truncate">{name}</div>
                                                                            <div class="text-[9px] text-slate-400 font-mono truncate">{meta}</div>
                                                                        </div>
                                                                        <div class="flex items-center gap-1 shrink-0">
                                                                            <button
                                                                                class="px-2 py-0.5 text-[10px] bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 border border-emerald-300 dark:border-emerald-800 rounded cursor-pointer"
                                                                                on:click=move |_| do_restore(a_restore.id.clone())
                                                                            >
                                                                                "恢复"
                                                                            </button>
                                                                            <button
                                                                                class="px-2 py-0.5 text-[10px] text-red-600 hover:bg-red-50 dark:hover:bg-red-950/40 rounded cursor-pointer"
                                                                                on:click=move |_| pending_purge.set(Some(a_purge.clone()))
                                                                            >
                                                                                "彻底删除"
                                                                            </button>
                                                                        </div>
                                                                    </div>
                                                                }
                                                            }
                                                        />
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <span></span> }.into_any()
                                            }}
                                        </div>
                                    }.into_any()
                                }}
                                // 维护：全库范围的图片存储对账
                                <div class="mt-4 pt-2 border-t border-slate-200 dark:border-slate-700">
                                    <button
                                        class="text-[11px] text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 cursor-pointer"
                                        on:click=move |_| show_maintenance.set(!show_maintenance.get())
                                    >
                                        {move || format!(
                                            "{} 维护（图片存储对账）",
                                            if show_maintenance.get() { "▾" } else { "▸" },
                                        )}
                                    </button>
                                    {move || if show_maintenance.get() {
                                        view! {
                                            <div class="mt-1.5 space-y-1.5">
                                                <div class="text-[9px] text-slate-400 leading-relaxed">
                                                    "作用域是全库，不只是当前作品 —— 孤儿文件已经不属于任何作品。"
                                                </div>
                                                <button
                                                    class="px-2.5 py-1 text-[11px] bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 rounded cursor-pointer disabled:opacity-50"
                                                    disabled=move || is_scanning.get()
                                                    on:click=move |_| do_scan()
                                                >
                                                    {move || if is_scanning.get() { "⏳ 处理中" } else { "扫描图片存储" }}
                                                </button>
                                                {move || {
                                                    let Some(report) = scan_report.get() else {
                                                        return view! { <span></span> }.into_any();
                                                    };
                                                    let orphans = report.orphan_files.len();
                                                    let removable = report
                                                        .orphan_files
                                                        .iter()
                                                        .filter(|o| !o.referenced_by_content)
                                                        .count();
                                                    let bytes: i64 = report.orphan_files.iter().map(|o| o.size).sum();
                                                    let missing = report.missing_records.len();
                                                    view! {
                                                        <div class="space-y-1.5">
                                                            <div class="text-[10px] text-slate-500 dark:text-slate-400">
                                                                {format!(
                                                                    "孤儿文件 {} 个（合计 {}，其中可清理 {} 个）· 记录缺失 {} 条",
                                                                    orphans, format_size(bytes), removable, missing,
                                                                )}
                                                            </div>
                                                            <div class="space-y-1">
                                                                <For
                                                                    each=move || scan_report
                                                                        .get()
                                                                        .map(|r| r.orphan_files)
                                                                        .unwrap_or_default()
                                                                    key=|o| o.rel_path.clone()
                                                                    children=move |o| {
                                                                        let rel = o.rel_path.clone();
                                                                        let size = format_size(o.size);
                                                                        let blocked = o.referenced_by_content;
                                                                        view! {
                                                                            <div class="flex items-center justify-between gap-2 p-1.5 rounded border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/60">
                                                                                <div class="min-w-0">
                                                                                    <div class="text-[9px] text-slate-500 dark:text-slate-400 font-mono truncate">{rel}</div>
                                                                                    <div class="text-[9px] text-slate-400">{size}</div>
                                                                                </div>
                                                                                {if blocked {
                                                                                    view! {
                                                                                        <span class="text-[9px] px-1.5 py-0.5 rounded bg-amber-100 dark:bg-amber-950/50 text-amber-700 dark:text-amber-300 shrink-0">
                                                                                            "仍被正文引用 · 不清理"
                                                                                        </span>
                                                                                    }.into_any()
                                                                                } else {
                                                                                    view! {
                                                                                        <span class="text-[9px] px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-slate-500 shrink-0">
                                                                                            "可清理"
                                                                                        </span>
                                                                                    }.into_any()
                                                                                }}
                                                                            </div>
                                                                        }
                                                                    }
                                                                />
                                                            </div>
                                                            <div class="space-y-1">
                                                                <For
                                                                    each=move || scan_report
                                                                        .get()
                                                                        .map(|r| r.missing_records)
                                                                        .unwrap_or_default()
                                                                    key=|a| a.id.clone()
                                                                    children=move |a| {
                                                                        let name = a.name.clone();
                                                                        let rel = a.rel_path.clone();
                                                                        view! {
                                                                            <div class="p-1.5 rounded border border-amber-200 dark:border-amber-900/60 bg-amber-50 dark:bg-amber-950/30">
                                                                                <div class="text-[9px] text-amber-700 dark:text-amber-300 truncate">"⚠ " {name}</div>
                                                                                <div class="text-[9px] text-amber-600/80 dark:text-amber-400/80 font-mono truncate">{rel}</div>
                                                                            </div>
                                                                        }
                                                                    }
                                                                />
                                                            </div>
                                                            <button
                                                                class="px-2.5 py-1 text-[11px] text-red-600 dark:text-red-400 border border-red-200 dark:border-red-900/60 hover:bg-red-50 dark:hover:bg-red-950/40 rounded cursor-pointer disabled:opacity-40"
                                                                disabled=move || is_scanning.get() || removable == 0
                                                                on:click=move |_| do_prune()
                                                            >
                                                                "清理可清理的孤儿文件"
                                                            </button>
                                                        </div>
                                                    }.into_any()
                                                }}
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
                                </div>
                            </div>

                            // 详情
                            <div class="w-64 shrink-0 border-l border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/60 overflow-y-auto p-3">
                                {move || {
                                    let Some(a) = selected_id
                                        .get()
                                        .and_then(|id| assets.get().into_iter().find(|x| x.id == id))
                                    else {
                                        return view! {
                                            <div class="text-[11px] text-slate-400 leading-relaxed">
                                                "选中一张图片可查看详情、重命名，或把它插回正文。"
                                            </div>
                                        }.into_any();
                                    };
                                    let a_insert = a.clone();
                                    let a_copy = a.clone();
                                    let a_remove = a.clone();
                                    let url = crate::bridge::asset::resolve_asset_url(&a.rel_path, &app_dir.get());
                                    let hash_prefix: String = a.hash.chars().take(12).collect();
                                    let missing = broken.get().contains(&a.rel_path);
                                    view! {
                                        <div class="space-y-2">
                                            <div class="rounded-md overflow-hidden border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800">
                                                <img class="w-full max-h-40 object-contain" src=url alt=a.name.clone() />
                                            </div>
                                            {if missing {
                                                view! {
                                                    <div class="text-[10px] p-1.5 rounded bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800 text-amber-700 dark:text-amber-300">
                                                        "⚠ 记录在库，但磁盘上找不到该文件（可能是 WebDAV 恢复未补齐）"
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <span></span> }.into_any()
                                            }}
                                            <div class="text-[11px] space-y-1">
                                                <div class="font-semibold text-slate-800 dark:text-slate-100 break-all">{a.name.clone()}</div>
                                                <div class="text-slate-500 dark:text-slate-400">{a.mime.clone()} " · " {format_size(a.size)}</div>
                                                <div class="text-slate-500 dark:text-slate-400">{a.created_at.clone()}</div>
                                                <div class="text-slate-400 dark:text-slate-500 font-mono text-[10px] break-all">{hash_prefix}</div>
                                                <div class="text-slate-400 dark:text-slate-500 font-mono text-[10px] break-all">{a.rel_path.clone()}</div>
                                            </div>

                                            // 引用规模：删除前应该知道的事
                                            <div class="text-[10px] text-slate-500 dark:text-slate-400 leading-relaxed border-t border-slate-200 dark:border-slate-700 pt-1.5">
                                                {move || {
                                                    let u = usage.get();
                                                    format!(
                                                        "正文引用 {} 章 · 历史版本引用 {} 条 · 同文件另有 {} 条记录",
                                                        u.referencing_chapters,
                                                        u.referencing_revisions,
                                                        u.shared_records,
                                                    )
                                                }}
                                            </div>

                                            <div class="space-y-1.5 pt-1">
                                                <button
                                                    class="w-full px-2.5 py-1.5 text-[11px] bg-blue-600 hover:bg-blue-700 text-white rounded font-medium transition-colors cursor-pointer"
                                                    on:click=move |_| on_insert(a_insert.clone())
                                                >
                                                    "插入到光标处"
                                                </button>
                                                <button
                                                    class="w-full px-2.5 py-1.5 text-[11px] bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded transition-colors cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-700"
                                                    on:click=move |_| on_copy(a_copy.clone())
                                                >
                                                    "复制路径"
                                                </button>
                                            </div>

                                            // 重命名（只改显示名）
                                            <div class="space-y-1 border-t border-slate-200 dark:border-slate-700 pt-2">
                                                <div class="text-[10px] text-slate-500 dark:text-slate-400">"重命名（不影响正文与文件）"</div>
                                                <div class="flex items-center gap-1">
                                                    <input
                                                        type="text"
                                                        class="flex-1 min-w-0 p-1 text-[11px] border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                                                        prop:value=move || rename_text.get()
                                                        on:input=move |ev| rename_text.set(event_target_value(&ev))
                                                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                            if ev.key() == "Enter" {
                                                                do_rename();
                                                            }
                                                        }
                                                    />
                                                    <button
                                                        class="px-2 py-1 text-[11px] bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 rounded cursor-pointer shrink-0"
                                                        on:click=move |_| do_rename()
                                                    >
                                                        "保存"
                                                    </button>
                                                </div>
                                            </div>

                                            // 移除（软删除，可恢复）
                                            <div class="border-t border-slate-200 dark:border-slate-700 pt-2">
                                                <button
                                                    class="w-full px-2.5 py-1.5 text-[11px] text-red-600 dark:text-red-400 border border-red-200 dark:border-red-900/60 hover:bg-red-50 dark:hover:bg-red-950/40 rounded transition-colors cursor-pointer"
                                                    on:click=move |_| pending_remove.set(Some(a_remove.clone()))
                                                >
                                                    "从图库移除"
                                                </button>
                                                <div class="text-[9px] text-slate-400 mt-1 leading-relaxed">
                                                    "只移除记录、不删磁盘文件，可恢复；正文里的显示不受影响。"
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                    </div>
                </div>

                // 确认移除（带引用规模提示）
                {move || {
                    let Some(target) = pending_remove.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    let id = target.id.clone();
                    let name = target.name.clone();
                    let u = usage.get();
                    view! {
                        <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-60 p-6">
                            <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-md w-full border border-slate-200 dark:border-slate-700 overflow-hidden">
                                <div class="px-5 py-3 bg-slate-50 dark:bg-slate-900/60 border-b border-slate-200 dark:border-slate-700 flex items-center gap-2">
                                    <span class="text-lg">"🗂"</span>
                                    <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100">"从图库移除"</h3>
                                </div>
                                <div class="p-5 space-y-2 text-xs text-slate-700 dark:text-slate-300">
                                    <p>"即将移除《" <strong>{name}</strong> "》。"</p>
                                    <p class="text-slate-500 dark:text-slate-400">
                                        "记录会移入「已移除的图片」，可随时恢复；磁盘文件不会被删除。"
                                    </p>
                                    {if u.referencing_chapters > 0 || u.referencing_revisions > 0 {
                                        view! {
                                            <p class="p-2 rounded bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800 text-amber-700 dark:text-amber-300">
                                                {format!(
                                                    "该图仍被 {} 章正文、{} 条历史版本引用；移除记录不影响它们的显示，但该图将不再进入 WebDAV 的补图清单。",
                                                    u.referencing_chapters, u.referencing_revisions,
                                                )}
                                            </p>
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
                                </div>
                                <div class="px-5 py-3 bg-slate-50 dark:bg-slate-900/60 border-t border-slate-200 dark:border-slate-700 flex items-center justify-end gap-2">
                                    <button
                                        class="px-3.5 py-1.5 bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded font-medium transition-colors cursor-pointer"
                                        on:click=move |_| pending_remove.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="px-3.5 py-1.5 bg-red-600 hover:bg-red-700 text-white rounded font-medium transition-colors cursor-pointer"
                                        on:click=move |_| {
                                            pending_remove.set(None);
                                            do_remove(id.clone());
                                        }
                                    >
                                        "确认移除"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }}

                // 确认彻底删除（不可恢复）
                {move || {
                    let Some(target) = pending_purge.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    let id = target.id.clone();
                    let name = target.name.clone();
                    view! {
                        <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-60 p-6">
                            <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-md w-full border border-red-200 dark:border-red-800 overflow-hidden">
                                <div class="px-5 py-3 bg-red-50 dark:bg-red-950/40 border-b border-red-200 dark:border-red-800 flex items-center gap-2">
                                    <span class="text-lg">"⚠️"</span>
                                    <h3 class="font-bold text-sm text-red-700 dark:text-red-300">"彻底删除记录"</h3>
                                </div>
                                <div class="p-5 space-y-2 text-xs text-slate-700 dark:text-slate-300">
                                    <p>"即将彻底删除《" <strong>{name}</strong> "》的记录，此操作不可恢复。"</p>
                                    <p class="text-slate-500 dark:text-slate-400">
                                        "磁盘文件不会被立即删除——它会在「维护」里被识别为孤儿文件，由你确认后清理（那里会先检查是否仍被正文或历史版本引用）。"
                                    </p>
                                </div>
                                <div class="px-5 py-3 bg-slate-50 dark:bg-slate-900/60 border-t border-slate-200 dark:border-slate-700 flex items-center justify-end gap-2">
                                    <button
                                        class="px-3.5 py-1.5 bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded font-medium transition-colors cursor-pointer"
                                        on:click=move |_| pending_purge.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="px-3.5 py-1.5 bg-red-600 hover:bg-red-700 text-white rounded font-medium transition-colors cursor-pointer"
                                        on:click=move |_| {
                                            pending_purge.set(None);
                                            do_purge(id.clone());
                                        }
                                    >
                                        "确认彻底删除"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }}
            }.into_any()
        }}
    }
}

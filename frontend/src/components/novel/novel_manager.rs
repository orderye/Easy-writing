use crate::state::AppContext;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use crate::workspace::{load_novel_workspace, refresh_novels_list};
use leptos::prelude::*;
use shared_types::{Novel, NovelWithStats, TrashItem};

/// 格式化字数显示：达到万字时转为友好万字显示
fn format_word_count(words: i64) -> String {
    if words >= 10_000 {
        format!("{:.1} 万字", words as f64 / 10000.0)
    } else {
        format!("{} 字", words)
    }
}

/// 格式化大数值千分位
fn format_number_commas(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, c) in chars.into_iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// 作品管理中心：看板式多作品概览、编辑名称/简介、卷章字数统计、实时检索与一键切换。
#[component]
pub fn NovelManagerModal(app_ctx: AppContext) -> impl IntoView {
    let is_open = app_ctx.is_novel_manager_open;

    // 核心数据状态
    let novels = RwSignal::new(Vec::<NovelWithStats>::new());
    let deleted_novels = RwSignal::new(Vec::<TrashItem>::new());
    let search_query = RwSignal::new(String::new());

    // 表单状态：新建
    let is_create_open = RwSignal::new(false);
    let new_title = RwSignal::new(String::new());
    let new_synopsis = RwSignal::new(String::new());

    // 表单状态：编辑 (id, title, synopsis)
    let editing_novel = RwSignal::new(Option::<(String, String, String)>::None);

    // 弹窗状态：删除确认 (id, title)
    let pending_delete = RwSignal::new(Option::<(String, String)>::None);

    // 折叠状态：已删除作品列表
    let show_deleted = RwSignal::new(false);

    let status_msg = RwSignal::new(String::new());
    let is_busy = RwSignal::new(false);

    // 载入作品带统计及已删除作品
    let load_data = move || {
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_no_args::<Vec<NovelWithStats>>("list_novels_with_stats").await {
                let simple_novels: Vec<Novel> = list.iter().map(|n| n.to_novel()).collect();
                app_ctx.novels_list.set(simple_novels);
                novels.set(list);
            } else {
                let simple_novels = refresh_novels_list(app_ctx).await;
                let fallback_list = simple_novels
                    .into_iter()
                    .map(|n| NovelWithStats {
                        id: n.id,
                        title: n.title,
                        synopsis: n.synopsis,
                        created_at: n.created_at,
                        updated_at: n.updated_at,
                        volume_count: 0,
                        chapter_count: 0,
                        word_count: 0,
                    })
                    .collect();
                novels.set(fallback_list);
            }
            if let Ok(del_list) = invoke_no_args::<Vec<TrashItem>>("list_deleted_novels").await {
                deleted_novels.set(del_list);
            }
        });
    };

    // 弹窗打开时刷新
    Effect::new(move |_| {
        if is_open.get() {
            status_msg.set(String::new());
            search_query.set(String::new());
            pending_delete.set(None);
            editing_novel.set(None);
            is_create_open.set(false);
            load_data();
        }
    });

    // 汇总指标计算
    let total_novels_count = move || novels.get().len();
    let total_words_count = move || novels.get().iter().map(|n| n.word_count).sum::<i64>();
    let total_chapters_count = move || novels.get().iter().map(|n| n.chapter_count).sum::<i64>();
    let total_volumes_count = move || novels.get().iter().map(|n| n.volume_count).sum::<i64>();

    // 搜索过滤
    let filtered_novels = move || {
        let q = search_query.get().trim().to_lowercase();
        let all = novels.get();
        if q.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|n| {
                    n.title.to_lowercase().contains(&q)
                        || n.synopsis.to_lowercase().contains(&q)
                })
                .collect()
        }
    };

    // 新建作品逻辑
    let create_novel_now = move || {
        let title = new_title.get().trim().to_string();
        let synopsis = new_synopsis.get().trim().to_string();
        if title.is_empty() {
            status_msg.set("✕ 请输入作品名称".to_string());
            return;
        }
        is_busy.set(true);
        leptos::task::spawn_local(async move {
            match invoke_cmd("create_novel")
                .arg("title", title.clone())
                .arg("synopsis", synopsis)
                .send::<Novel>()
                .await
            {
                Ok(novel) => {
                    new_title.set(String::new());
                    new_synopsis.set(String::new());
                    is_create_open.set(false);
                    load_data();
                    // 新建后直接切换过去，并初始化卷章
                    load_novel_workspace(app_ctx, novel, true).await;
                    status_msg.set(format!("✓ 已新建并切换到《{}》", title));
                }
                Err(e) => status_msg.set(format!("✕ 新建失败：{}", e)),
            }
            is_busy.set(false);
        });
    };

    // 保存编辑作品信息
    let save_edit_now = move || {
        let Some((id, title, synopsis)) = editing_novel.get() else {
            return;
        };
        let title = title.trim().to_string();
        let synopsis = synopsis.trim().to_string();
        if title.is_empty() {
            status_msg.set("✕ 作品名称不能为空".to_string());
            return;
        }
        is_busy.set(true);
        leptos::task::spawn_local(async move {
            match invoke_cmd("update_novel")
                .arg("id", id.clone())
                .arg("title", title.clone())
                .arg("synopsis", synopsis.clone())
                .send_unit()
                .await
            {
                Ok(_) => {
                    // 若修改的是当前活跃作品，实时同步 app_ctx.current_novel
                    if let Some(curr) = app_ctx.current_novel.get() {
                        if curr.id == id {
                            app_ctx.current_novel.set(Some(Novel {
                                id: curr.id,
                                title: title.clone(),
                                synopsis: synopsis.clone(),
                                created_at: curr.created_at,
                                updated_at: curr.updated_at,
                                deleted_at: curr.deleted_at,
                            }));
                        }
                    }
                    editing_novel.set(None);
                    load_data();
                    status_msg.set(format!("✓ 已更新作品《{}》信息", title));
                }
                Err(e) => status_msg.set(format!("✕ 保存修改失败：{}", e)),
            }
            is_busy.set(false);
        });
    };

    // 切换作品
    let on_switch = move |novel_stats: NovelWithStats| {
        if app_ctx.save_status.get() == "dirty" {
            status_msg.set("✕ 当前章节有未保存修改，请先保存后再切换作品".to_string());
            return;
        }
        is_busy.set(true);
        let title = novel_stats.title.clone();
        let novel = novel_stats.to_novel();
        leptos::task::spawn_local(async move {
            load_novel_workspace(app_ctx, novel, false).await;
            load_data();
            status_msg.set(format!("✓ 已切换到《{}》", title));
            is_busy.set(false);
        });
    };

    // 确认删除作品
    let on_confirm_delete = move |_| {
        let Some((id, title)) = pending_delete.get() else {
            return;
        };
        is_busy.set(true);
        leptos::task::spawn_local(async move {
            match invoke_cmd("delete_novel").arg("id", id.clone()).send_unit().await {
                Ok(_) => {
                    load_data();
                    // 若删的是当前作品，切到剩下的第一个（没有则清空工作区）
                    let deleting_current = app_ctx
                        .current_novel
                        .get()
                        .map(|c| c.id == id)
                        .unwrap_or(false);
                    if deleting_current {
                        let novels_list = app_ctx.novels_list.get();
                        let remaining: Vec<Novel> = novels_list.into_iter().filter(|n| n.id != id).collect();
                        match remaining.first() {
                            Some(next) => {
                                let next = next.clone();
                                load_novel_workspace(app_ctx, next, true).await;
                            }
                            None => {
                                app_ctx.current_novel.set(None);
                                app_ctx.volumes_list.set(Vec::new());
                                app_ctx.current_volume.set(None);
                                app_ctx.chapters_map.set(Vec::new());
                                app_ctx.current_chapter.set(None);
                                app_ctx.word_count.set(0);
                            }
                        }
                    }
                    pending_delete.set(None);
                    status_msg.set(format!(
                        "✓ 已删除《{}》，可在下方「已删除作品」中恢复",
                        title
                    ));
                }
                Err(e) => status_msg.set(format!("✕ 删除失败：{}", e)),
            }
            is_busy.set(false);
        });
    };

    // 恢复已删除作品
    let on_restore_deleted = move |id: String| {
        is_busy.set(true);
        leptos::task::spawn_local(async move {
            let res = invoke_cmd("restore_trash_item")
                .arg("item_type", "novel".to_string())
                .arg("item_id", id)
                .send_unit()
                .await;
            match res {
                Ok(_) => {
                    load_data();
                    status_msg.set("✓ 作品已恢复".to_string());
                }
                Err(e) => status_msg.set(format!("✕ 恢复失败：{}", e)),
            }
            is_busy.set(false);
        });
    };

    view! {
        {move || {
            if !is_open.get() {
                return view! { <div></div> }.into_any();
            }
            view! {
                <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-50 p-4 sm:p-6 animate-in fade-in duration-150">
                    <div class="bg-white dark:bg-slate-900 rounded-2xl shadow-2xl max-w-3xl w-full border border-slate-200 dark:border-slate-800 overflow-hidden flex flex-col max-h-[90vh]">
                        // 1. 顶部标题栏
                        <div class="px-6 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50/80 dark:bg-slate-850/80">
                            <div class="flex items-center gap-3">
                                <div class="w-9 h-9 rounded-xl bg-blue-100 dark:bg-blue-950/60 border border-blue-200 dark:border-blue-800 flex items-center justify-center text-lg">
                                    "📚"
                                </div>
                                <div>
                                    <h3 class="font-bold text-base text-slate-800 dark:text-slate-100 flex items-center gap-2">
                                        "作品管理中心"
                                        <span class="text-xs font-normal text-slate-400 dark:text-slate-500">
                                            {move || format!("共 {} 部作品", total_novels_count())}
                                        </span>
                                    </h3>
                                    <p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5">
                                        "查看全书结构与字数统计，支持修改作品信息、快速检索与无缝切换"
                                    </p>
                                </div>
                            </div>
                            <button
                                class="w-8 h-8 rounded-lg flex items-center justify-center text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors cursor-pointer text-lg font-mono"
                                title="关闭"
                                on:click=move |_| is_open.set(false)
                            >
                                "✕"
                            </button>
                        </div>

                        // 2. 统计指标看板 (KPI Banner)
                        <div class="px-6 py-3.5 bg-slate-100/50 dark:bg-slate-850/40 border-b border-slate-200/80 dark:border-slate-800 grid grid-cols-4 gap-3 text-center">
                            <div class="bg-white dark:bg-slate-800/80 p-2.5 rounded-xl border border-slate-200/60 dark:border-slate-700/60 shadow-2xs">
                                <div class="text-[11px] text-slate-400 dark:text-slate-500 font-medium">"作品总数"</div>
                                <div class="text-base font-bold text-slate-800 dark:text-slate-100 mt-0.5">
                                    {move || total_novels_count()} <span class="text-xs font-normal text-slate-400">"部"</span>
                                </div>
                            </div>
                            <div class="bg-white dark:bg-slate-800/80 p-2.5 rounded-xl border border-slate-200/60 dark:border-slate-700/60 shadow-2xs">
                                <div class="text-[11px] text-slate-400 dark:text-slate-500 font-medium">"累计创作字数"</div>
                                <div class="text-base font-bold text-blue-600 dark:text-blue-400 mt-0.5">
                                    {move || format_number_commas(total_words_count())} <span class="text-xs font-normal text-slate-400">"字"</span>
                                </div>
                            </div>
                            <div class="bg-white dark:bg-slate-800/80 p-2.5 rounded-xl border border-slate-200/60 dark:border-slate-700/60 shadow-2xs">
                                <div class="text-[11px] text-slate-400 dark:text-slate-500 font-medium">"总篇章数"</div>
                                <div class="text-base font-bold text-slate-800 dark:text-slate-100 mt-0.5">
                                    {move || total_chapters_count()} <span class="text-xs font-normal text-slate-400">"章"</span>
                                </div>
                            </div>
                            <div class="bg-white dark:bg-slate-800/80 p-2.5 rounded-xl border border-slate-200/60 dark:border-slate-700/60 shadow-2xs">
                                <div class="text-[11px] text-slate-400 dark:text-slate-500 font-medium">"总卷数"</div>
                                <div class="text-base font-bold text-slate-800 dark:text-slate-100 mt-0.5">
                                    {move || total_volumes_count()} <span class="text-xs font-normal text-slate-400">"卷"</span>
                                </div>
                            </div>
                        </div>

                        // 3. 主体内容区（滚动）
                        <div class="flex-1 overflow-y-auto p-6 space-y-4 text-xs">
                            // 状态反馈浮条
                            {move || {
                                let msg = status_msg.get();
                                if msg.is_empty() {
                                    return view! { <div></div> }.into_any();
                                }
                                let cls = if msg.starts_with('✓') {
                                    "bg-emerald-50 dark:bg-emerald-950/40 border-emerald-200 dark:border-emerald-800 text-emerald-800 dark:text-emerald-300"
                                } else {
                                    "bg-red-50 dark:bg-red-950/40 border-red-200 dark:border-red-800 text-red-800 dark:text-red-300"
                                };
                                view! {
                                    <div class=format!("p-3 rounded-xl border font-medium flex items-center justify-between {}", cls)>
                                        <span>{msg}</span>
                                        <button
                                            class="text-xs opacity-60 hover:opacity-100 cursor-pointer ml-2"
                                            on:click=move |_| status_msg.set(String::new())
                                        >
                                            "✕"
                                        </button>
                                    </div>
                                }.into_any()
                            }}

                            // 操作栏：搜索框 + 新建作品触发按钮
                            <div class="flex items-center gap-3">
                                <div class="relative flex-1">
                                    <span class="absolute left-3 top-2.5 text-slate-400 text-xs pointer-events-none select-none">"🔍"</span>
                                    <input
                                        type="text"
                                        class="w-full text-xs pl-8 pr-8 py-2 bg-slate-50 dark:bg-slate-800/80 border border-slate-200 dark:border-slate-700 rounded-xl text-slate-800 dark:text-slate-100 placeholder-slate-400 focus:outline-hidden focus:ring-2 focus:ring-blue-500/40 focus:border-blue-500 transition-all"
                                        placeholder="检索作品名称、题材或简介内容..."
                                        prop:value=move || search_query.get()
                                        on:input=move |ev| search_query.set(event_target_value(&ev))
                                    />
                                    {move || {
                                        if !search_query.get().is_empty() {
                                            view! {
                                                <button
                                                    class="absolute right-3 top-2.5 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 text-xs cursor-pointer select-none"
                                                    title="清空检索"
                                                    on:click=move |_| search_query.set(String::new())
                                                >
                                                    "✕"
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! { <span class="hidden"></span> }.into_any()
                                        }
                                    }}
                                </div>
                                <button
                                    class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-xl font-medium transition-all cursor-pointer shadow-sm hover:shadow flex items-center gap-1.5 shrink-0"
                                    on:click=move |_| is_create_open.set(!is_create_open.get())
                                >
                                    <span>{move || if is_create_open.get() { "✕ 收起" } else { "＋ 新建作品" }}</span>
                                </button>
                            </div>

                            // 折叠式新建表单
                            {move || {
                                if !is_create_open.get() {
                                    return view! { <div></div> }.into_any();
                                }
                                view! {
                                    <div class="p-4 bg-blue-50/50 dark:bg-blue-950/20 border border-blue-200/80 dark:border-blue-800/60 rounded-2xl space-y-3 animate-in fade-in duration-150">
                                        <div class="flex items-center justify-between">
                                            <span class="font-bold text-xs text-blue-900 dark:text-blue-300 flex items-center gap-1.5">
                                                <span>"✨"</span> "新建作品档案"
                                            </span>
                                            <span class="text-[11px] text-slate-400">"新建后将自动为你生成第一卷和第一章并载入工作台"</span>
                                        </div>
                                        <div class="space-y-2">
                                            <input
                                                type="text"
                                                class="w-full p-2.5 border border-slate-300 dark:border-slate-600 rounded-xl bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-2 focus:ring-blue-500/40 text-xs font-medium"
                                                placeholder="输入新作品名称（必填，如《星海纪元》）..."
                                                prop:value=move || new_title.get()
                                                on:input=move |ev| new_title.set(event_target_value(&ev))
                                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                    if ev.key() == "Enter" {
                                                        create_novel_now();
                                                    }
                                                }
                                            />
                                            <textarea
                                                rows="2"
                                                class="w-full p-2.5 border border-slate-300 dark:border-slate-600 rounded-xl bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-2 focus:ring-blue-500/40 text-xs resize-none"
                                                placeholder="作品简介（选填：故事核心设定、主角目标、题材风格等）..."
                                                prop:value=move || new_synopsis.get()
                                                on:input=move |ev| new_synopsis.set(event_target_value(&ev))
                                            ></textarea>
                                        </div>
                                        <div class="flex items-center justify-end gap-2">
                                            <button
                                                class="px-3 py-1.5 rounded-lg border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300 font-medium transition-colors cursor-pointer text-xs"
                                                on:click=move |_| is_create_open.set(false)
                                            >
                                                "取消"
                                            </button>
                                            <button
                                                class="px-4 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition-colors cursor-pointer disabled:opacity-50 text-xs flex items-center gap-1"
                                                disabled=move || is_busy.get()
                                                on:click=move |_| create_novel_now()
                                            >
                                                "确认创建并打开"
                                            </button>
                                        </div>
                                    </div>
                                }.into_any()
                            }}

                            // 作品卡片列表
                            <div class="space-y-3">
                                {move || {
                                    let list = filtered_novels();
                                    if list.is_empty() {
                                        return view! {
                                            <div class="p-10 text-center rounded-2xl border border-dashed border-slate-200 dark:border-slate-800 bg-slate-50/50 dark:bg-slate-850/20 text-slate-400">
                                                <div class="text-3xl mb-2">"📭"</div>
                                                <p class="font-medium text-xs">
                                                    {if search_query.get().is_empty() {
                                                        "还没有作品，点击上方「＋ 新建作品」开启你的第一部小说创作！"
                                                    } else {
                                                        "未找到匹配的作品，请尝试更换关键词搜索"
                                                    }}
                                                </p>
                                            </div>
                                        }.into_any();
                                    }
                                    view! {
                                        <div class="space-y-2.5">
                                            <For
                                                each=filtered_novels
                                                key=|n| n.id.clone()
                                                children=move |novel_stats| {
                                                    let is_current = {
                                                        let id = novel_stats.id.clone();
                                                        move || app_ctx
                                                            .current_novel
                                                            .get()
                                                            .map(|c| c.id == id)
                                                            .unwrap_or(false)
                                                    };
                                                    let is_curr_cls = is_current.clone();
                                                    let is_curr_badge = is_current.clone();
                                                    let is_curr_switch = is_current.clone();

                                                    let n_switch = novel_stats.clone();
                                                    let n_edit = novel_stats.clone();
                                                    let del_id = novel_stats.id.clone();
                                                    let del_title = novel_stats.title.clone();

                                                    // 作品头像/封面首字
                                                    let first_char = novel_stats.title.chars().next().unwrap_or('书').to_string();

                                                    view! {
                                                        <div class=move || {
                                                            format!(
                                                                "p-4 rounded-2xl border transition-all duration-150 flex flex-col sm:flex-row sm:items-center justify-between gap-4 {}",
                                                                if is_curr_cls() {
                                                                    "border-blue-400/80 bg-blue-50/60 dark:bg-blue-950/30 dark:border-blue-700/80 shadow-xs"
                                                                } else {
                                                                    "border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-850 hover:border-slate-300 dark:hover:border-slate-700 hover:shadow-2xs"
                                                                }
                                                            )
                                                        }>
                                                            // 左侧：封面书脊 + 标题与简介
                                                            <div class="flex items-start gap-3.5 min-w-0 flex-1">
                                                                // 模拟小书脊封面
                                                                <div class="w-10 h-13 rounded-lg bg-gradient-to-br from-blue-600 to-indigo-800 text-white font-bold text-sm flex items-center justify-center shrink-0 shadow-sm border border-blue-400/30 select-none">
                                                                    {first_char}
                                                                </div>

                                                                <div class="min-w-0 flex-1 space-y-1.5">
                                                                    <div class="flex items-center gap-2 flex-wrap">
                                                                        <h4 class="font-bold text-sm text-slate-800 dark:text-slate-100 truncate">
                                                                            {novel_stats.title.clone()}
                                                                        </h4>
                                                                        {move || if is_curr_badge() {
                                                                            view! {
                                                                                <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-blue-600 text-white shrink-0 shadow-2xs">
                                                                                    "当前创作中"
                                                                                </span>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! { <span></span> }.into_any()
                                                                        }}
                                                                    </div>

                                                                    // 作品简介预览
                                                                    <p class="text-[11px] text-slate-500 dark:text-slate-400 line-clamp-2 leading-relaxed">
                                                                        {if novel_stats.synopsis.trim().is_empty() {
                                                                            "暂无作品简介，点击「编辑」补充背景设定与核心概要...".to_string()
                                                                        } else {
                                                                            novel_stats.synopsis.clone()
                                                                        }}
                                                                    </p>

                                                                    // 指标标签行
                                                                    <div class="flex items-center gap-3 text-[11px] text-slate-400 dark:text-slate-500 pt-0.5 flex-wrap">
                                                                        <span class="flex items-center gap-1 font-medium text-slate-600 dark:text-slate-300">
                                                                            <span>"📊"</span> {format_word_count(novel_stats.word_count)}
                                                                        </span>
                                                                        <span class="text-slate-300 dark:text-slate-700">"|"</span>
                                                                        <span>{novel_stats.volume_count} " 卷"</span>
                                                                        <span class="text-slate-300 dark:text-slate-700">"|"</span>
                                                                        <span>{novel_stats.chapter_count} " 章"</span>
                                                                        <span class="text-slate-300 dark:text-slate-700">"|"</span>
                                                                        <span>"更新于 " {novel_stats.updated_at.clone()}</span>
                                                                    </div>
                                                                </div>
                                                            </div>

                                                            // 右侧：操作动作组
                                                            <div class="flex items-center gap-2 shrink-0 self-end sm:self-center">
                                                                // 切换按钮
                                                                {move || {
                                                                    if is_curr_switch() {
                                                                        view! {
                                                                            <button
                                                                                class="px-3 py-1.5 rounded-xl bg-blue-100/70 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300 font-semibold cursor-default text-xs"
                                                                                disabled=true
                                                                            >
                                                                                "✓ 当前"
                                                                            </button>
                                                                        }.into_any()
                                                                    } else {
                                                                        let n_sw = n_switch.clone();
                                                                        view! {
                                                                            <button
                                                                                class="px-3 py-1.5 rounded-xl border border-blue-500 text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-950/60 font-medium transition-colors cursor-pointer text-xs"
                                                                                disabled=move || is_busy.get()
                                                                                title="切换到这部作品"
                                                                                on:click=move |_| on_switch(n_sw.clone())
                                                                            >
                                                                                "切换"
                                                                            </button>
                                                                        }.into_any()
                                                                    }
                                                                }}

                                                                // 编辑元数据按钮
                                                                <button
                                                                    class="px-2.5 py-1.5 rounded-xl border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300 transition-colors cursor-pointer text-xs font-medium"
                                                                    title="修改作品名称与简介"
                                                                    on:click=move |_| {
                                                                        editing_novel.set(Some((
                                                                            n_edit.id.clone(),
                                                                            n_edit.title.clone(),
                                                                            n_edit.synopsis.clone(),
                                                                        )));
                                                                    }
                                                                >
                                                                    "✏️ 编辑"
                                                                </button>

                                                                // 软删除按钮
                                                                <button
                                                                    class="px-2.5 py-1.5 rounded-xl text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950/40 transition-colors cursor-pointer text-xs font-medium"
                                                                    title="移入废弃回收站（可随时恢复）"
                                                                    on:click=move |_| {
                                                                        pending_delete.set(Some((del_id.clone(), del_title.clone())));
                                                                    }
                                                                >
                                                                    "删除"
                                                                </button>
                                                            </div>
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    }.into_any()
                                }}
                            </div>

                            // 底部折叠：已删除作品（回收站联动）
                            <div class="pt-4 border-t border-slate-200 dark:border-slate-800 space-y-3">
                                <button
                                    class="flex items-center gap-2 text-slate-500 hover:text-slate-800 dark:hover:text-slate-200 font-medium cursor-pointer"
                                    on:click=move |_| show_deleted.set(!show_deleted.get())
                                >
                                    <span>{move || if show_deleted.get() { "▼" } else { "▶" }}</span>
                                    <span>
                                        {move || format!("已删除作品回收站（{}）", deleted_novels.get().len())}
                                    </span>
                                </button>

                                {move || {
                                    if !show_deleted.get() {
                                        return view! { <div></div> }.into_any();
                                    }
                                    let list = deleted_novels.get();
                                    if list.is_empty() {
                                        return view! {
                                            <div class="text-[11px] text-slate-400 py-2 pl-4">
                                                "回收站暂无已删除的作品"
                                            </div>
                                        }.into_any();
                                    }
                                    view! {
                                        <div class="space-y-2 pl-2">
                                            <For
                                                each=move || deleted_novels.get()
                                                key=|t| t.id.clone()
                                                children=move |item| {
                                                    let rid = item.id.clone();
                                                    view! {
                                                        <div class="flex items-center justify-between p-2.5 rounded-xl border border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-850/50 opacity-80">
                                                            <div class="min-w-0">
                                                                <div class="font-medium text-slate-600 dark:text-slate-300 truncate line-through">
                                                                    {item.title.clone()}
                                                                </div>
                                                                <div class="text-[10px] text-slate-400">
                                                                    "删除于 " {item.deleted_at.clone()}
                                                                </div>
                                                            </div>
                                                            <button
                                                                class="px-3 py-1 bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 border border-emerald-300 dark:border-emerald-800 rounded-lg transition-colors cursor-pointer text-xs font-medium shrink-0"
                                                                disabled=move || is_busy.get()
                                                                on:click=move |_| on_restore_deleted(rid.clone())
                                                            >
                                                                "恢复"
                                                            </button>
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                    </div>
                </div>

                // 4. 编辑作品信息模态框
                {move || {
                    let Some((_id, title, synopsis)) = editing_novel.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    view! {
                        <div class="fixed inset-0 bg-slate-900/60 backdrop-blur-xs flex items-center justify-center z-60 p-4">
                            <div class="bg-white dark:bg-slate-900 rounded-2xl shadow-2xl max-w-md w-full border border-slate-200 dark:border-slate-800 overflow-hidden animate-in fade-in zoom-in-95 duration-150">
                                <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50 dark:bg-slate-850">
                                    <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100 flex items-center gap-1.5">
                                        <span>"✏️"</span> "编辑作品信息"
                                    </h3>
                                    <button
                                        class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 text-sm cursor-pointer"
                                        on:click=move |_| editing_novel.set(None)
                                    >
                                        "✕"
                                    </button>
                                </div>
                                <div class="p-5 space-y-3 text-xs">
                                    <div class="space-y-1">
                                        <label class="font-bold text-slate-700 dark:text-slate-300">"作品名称"</label>
                                        <input
                                            type="text"
                                            class="w-full p-2.5 border border-slate-300 dark:border-slate-600 rounded-xl bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-2 focus:ring-blue-500 font-medium"
                                            prop:value=title.clone()
                                            on:input=move |ev| {
                                                if let Some((id, _, syn)) = editing_novel.get() {
                                                    editing_novel.set(Some((id, event_target_value(&ev), syn)));
                                                }
                                            }
                                        />
                                    </div>
                                    <div class="space-y-1">
                                        <label class="font-bold text-slate-700 dark:text-slate-300">"作品简介"</label>
                                        <textarea
                                            rows="4"
                                            class="w-full p-2.5 border border-slate-300 dark:border-slate-600 rounded-xl bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none leading-relaxed"
                                            prop:value=synopsis.clone()
                                            on:input=move |ev| {
                                                if let Some((id, t, _)) = editing_novel.get() {
                                                    editing_novel.set(Some((id, t, event_target_value(&ev))));
                                                }
                                            }
                                        ></textarea>
                                    </div>
                                </div>
                                <div class="px-5 py-3.5 bg-slate-50 dark:bg-slate-850 border-t border-slate-200 dark:border-slate-800 flex items-center justify-end gap-2">
                                    <button
                                        class="px-3.5 py-1.5 rounded-xl border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300 font-medium transition-colors cursor-pointer text-xs"
                                        on:click=move |_| editing_novel.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="px-4 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded-xl font-medium transition-colors cursor-pointer disabled:opacity-50 text-xs"
                                        disabled=move || is_busy.get()
                                        on:click=move |_| save_edit_now()
                                    >
                                        "保存修改"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }}

                // 5. 删除二次确认模态框
                {move || {
                    let Some((_, title)) = pending_delete.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    view! {
                        <div class="fixed inset-0 bg-slate-900/60 backdrop-blur-xs flex items-center justify-center z-60 p-4">
                            <div class="bg-white dark:bg-slate-900 rounded-2xl shadow-2xl max-w-md w-full border border-red-200 dark:border-red-900/60 overflow-hidden animate-in fade-in zoom-in-95 duration-150">
                                <div class="px-5 py-3.5 bg-red-50 dark:bg-red-950/40 border-b border-red-200 dark:border-red-900/60 flex items-center gap-2">
                                    <span class="text-lg">"⚠️"</span>
                                    <h3 class="font-bold text-sm text-red-700 dark:text-red-300">
                                        "确认删除作品"
                                    </h3>
                                </div>
                                <div class="p-5 space-y-3 text-xs text-slate-700 dark:text-slate-300 leading-relaxed">
                                    <p>
                                        "即将把《" <strong class="text-slate-900 dark:text-slate-100">{title.clone()}</strong> "》移入废弃回收站。"
                                    </p>
                                    <p class="text-slate-500 dark:text-slate-400">
                                        "作品下辖的全部卷、章节、设定及历史版本将一并安全标记废弃，本地数据库内容完整保留，你可随时在底部「已删除作品」中一键恢复。"
                                    </p>
                                </div>
                                <div class="px-5 py-3.5 bg-slate-50 dark:bg-slate-850 border-t border-slate-200 dark:border-slate-800 flex items-center justify-end gap-2">
                                    <button
                                        class="px-3.5 py-1.5 rounded-xl border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300 font-medium transition-colors cursor-pointer text-xs"
                                        on:click=move |_| pending_delete.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="px-4 py-1.5 bg-red-600 hover:bg-red-700 text-white rounded-xl font-medium transition-colors cursor-pointer disabled:opacity-50 text-xs"
                                        disabled=move || is_busy.get()
                                        on:click=on_confirm_delete
                                    >
                                        "确认移入回收站"
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

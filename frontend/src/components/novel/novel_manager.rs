use crate::state::AppContext;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use crate::workspace::{load_novel_workspace, refresh_novels_list};
use leptos::prelude::*;
use shared_types::{Novel, TrashItem};

/// 作品管理器：新建、切换、删除作品。
///
/// 背景：`delete_novel` 此前后端有实现但前端从未调用——用户能新建作品却删不掉。
/// 但直接暴露删除有风险：回收站以「当前作品」为作用域，被删的作品不属于任何作品，
/// 常规回收站查不到，等于「删了就永久丢失」。
///
/// 因此这里与回收站配合：删除为软删除（内容保留），可恢复到回收站。
#[component]
pub fn NovelManagerModal(app_ctx: AppContext) -> impl IntoView {
    let is_open = app_ctx.is_novel_manager_open;
    let new_title = RwSignal::new(String::new());
    let status_msg = RwSignal::new(String::new());
    let deleted_novels = RwSignal::new(Vec::<TrashItem>::new());
    let pending_delete = RwSignal::new(Option::<(String, String)>::None); // (id, title)
    let is_busy = RwSignal::new(false);

    let novel_title = |n: &Novel| n.title.clone();

    // 载入已删除作品（跨作品作用域，供删除后确认与恢复）
    let load_deleted = move || {
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_no_args::<Vec<TrashItem>>("list_deleted_novels").await {
                deleted_novels.set(list);
            }
        });
    };

    Effect::new(move |_| {
        if is_open.get() {
            status_msg.set(String::new());
            pending_delete.set(None);
            load_deleted();
            leptos::task::spawn_local(async move {
                refresh_novels_list(app_ctx).await;
            });
        }
    });

    // 新建逻辑体：点击与回车共用（回车没有 MouseEvent，无法复用事件闭包）
    let create_novel_now = move || {
        let title = new_title.get().trim().to_string();
        if title.is_empty() {
            status_msg.set("✕ 请输入作品名称".to_string());
            return;
        }
        is_busy.set(true);
        leptos::task::spawn_local(async move {
            match invoke_cmd("create_novel")
                .arg("title", title.clone())
                .arg("synopsis", "".to_string())
                .send::<Novel>()
                .await
            {
                Ok(novel) => {
                    new_title.set(String::new());
                    refresh_novels_list(app_ctx).await;
                    // 新建后直接切换过去，并初始化卷章
                    load_novel_workspace(app_ctx, novel, true).await;
                    status_msg.set(format!("✓ 已新建并切换到《{}》", title));
                }
                Err(e) => status_msg.set(format!("✕ 新建失败：{}", e)),
            }
            is_busy.set(false);
        });
    };

    let on_create = move |_: web_sys::MouseEvent| create_novel_now();

    let on_switch = move |novel: Novel| {
        if app_ctx.save_status.get() == "dirty" {
            status_msg.set("✕ 当前章节有未保存修改，请先保存后再切换作品".to_string());
            return;
        }
        is_busy.set(true);
        let title = novel_title(&novel);
        leptos::task::spawn_local(async move {
            load_novel_workspace(app_ctx, novel, false).await;
            status_msg.set(format!("✓ 已切换到《{}》", title));
            is_busy.set(false);
        });
    };

    let on_confirm_delete = move |_| {
        let Some((id, title)) = pending_delete.get() else {
            return;
        };
        is_busy.set(true);
        leptos::task::spawn_local(async move {
            match invoke_cmd("delete_novel").arg("id", id.clone()).send_unit().await {
                Ok(_) => {
                    let novels = refresh_novels_list(app_ctx).await;
                    // 若删的是当前作品，切到剩下的第一个（没有则清空工作区）
                    let deleting_current = app_ctx
                        .current_novel
                        .get()
                        .map(|c| c.id == id)
                        .unwrap_or(false);
                    if deleting_current {
                        match novels.first() {
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
                    load_deleted();
                    status_msg.set(format!(
                        "✓ 已删除《{}》，可在下方或设置面板的「回收站」中恢复",
                        title
                    ));
                }
                Err(e) => status_msg.set(format!("✕ 删除失败：{}", e)),
            }
            is_busy.set(false);
        });
    };

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
                    refresh_novels_list(app_ctx).await;
                    load_deleted();
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
                <div class="fixed inset-0 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center z-50 p-6">
                    <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-2xl w-full border border-slate-200 dark:border-slate-700 overflow-hidden flex flex-col max-h-[85vh]">
                        // 标题栏
                        <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-700 flex items-center justify-between bg-slate-50 dark:bg-slate-800/80">
                            <div class="flex items-center gap-2">
                                <span class="text-lg">"📚"</span>
                                <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100">
                                    "作品管理"
                                </h3>
                            </div>
                            <button
                                class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 font-mono text-lg cursor-pointer"
                                on:click=move |_| is_open.set(false)
                            >
                                "✕"
                            </button>
                        </div>

                        <div class="flex-1 overflow-y-auto p-5 space-y-5 text-xs">
                            // 状态提示
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
                                    <div class=format!("p-2.5 rounded-md border font-semibold {}", cls)>
                                        {msg}
                                    </div>
                                }.into_any()
                            }}

                            // 新建作品
                            <div class="space-y-2">
                                <h4 class="font-bold text-slate-800 dark:text-slate-100">"新建作品"</h4>
                                <div class="flex items-center gap-2">
                                    <input
                                        type="text"
                                        class="flex-1 p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                                        placeholder="输入新作品名称..."
                                        prop:value=move || new_title.get()
                                        on:input=move |ev| new_title.set(event_target_value(&ev))
                                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                                            if ev.key() == "Enter" {
                                                // 触发与点击同一个动作；Enter 没有 MouseEvent，
                                                // 因此这里只复用新建逻辑体
                                                create_novel_now();
                                            }
                                        }
                                    />
                                    <button
                                        class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium transition-colors cursor-pointer disabled:opacity-50 whitespace-nowrap"
                                        disabled=move || is_busy.get()
                                        on:click=on_create
                                    >
                                        "＋ 新建"
                                    </button>
                                </div>
                            </div>

                            // 作品列表
                            <div class="space-y-2">
                                <h4 class="font-bold text-slate-800 dark:text-slate-100">
                                    {move || format!("我的作品（{}）", app_ctx.novels_list.get().len())}
                                </h4>
                                <div class="space-y-1.5">
                                    <For
                                        each=move || app_ctx.novels_list.get()
                                        key=|n| n.id.clone()
                                        children=move |novel| {
                                            let is_current = {
                                                let id = novel.id.clone();
                                                move || app_ctx
                                                    .current_novel
                                                    .get()
                                                    .map(|c| c.id == id)
                                                    .unwrap_or(false)
                                            };
                                            let is_current_label = is_current.clone();
                                            let is_current_disable = is_current.clone();
                                            let is_current_row = is_current.clone();
                                            let n_switch = novel.clone();
                                            let del_id = novel.id.clone();
                                            let del_title = novel.title.clone();
                                            view! {
                                                <div class=move || {
                                                    format!(
                                                        "flex items-center justify-between p-2.5 rounded-lg border {}",
                                                        if is_current_row() {
                                                            "border-blue-400 bg-blue-50 dark:bg-blue-950/50 dark:border-blue-700"
                                                        } else {
                                                            "border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900"
                                                        }
                                                    )
                                                }>
                                                    <div class="min-w-0">
                                                        <div class="flex items-center gap-2">
                                                            <span class="font-semibold text-slate-800 dark:text-slate-100 truncate">
                                                                {novel.title.clone()}
                                                            </span>
                                                            {move || if is_current_label() {
                                                                view! {
                                                                    <span class="text-[10px] px-1.5 py-0.5 rounded bg-blue-600 text-white shrink-0">
                                                                        "当前"
                                                                    </span>
                                                                }.into_any()
                                                            } else {
                                                                view! { <span></span> }.into_any()
                                                            }}
                                                        </div>
                                                        <div class="text-[10px] text-slate-400 mt-0.5">
                                                            "更新于 " {novel.updated_at.clone()}
                                                        </div>
                                                    </div>
                                                    <div class="flex items-center gap-1.5 shrink-0">
                                                        <button
                                                            class="px-2.5 py-1 bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 rounded transition-colors cursor-pointer disabled:opacity-40"
                                                            disabled=move || is_busy.get() || is_current_disable()
                                                            title="切换到这个作品"
                                                            on:click=move |_| on_switch(n_switch.clone())
                                                        >
                                                            "切换"
                                                        </button>
                                                        <button
                                                            class="px-2.5 py-1 text-red-600 hover:bg-red-50 dark:hover:bg-red-950/40 rounded transition-colors cursor-pointer"
                                                            title="删除这个作品（可恢复）"
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
                            </div>

                            // 已删除作品
                            <div class="space-y-2">
                                <h4 class="font-bold text-slate-800 dark:text-slate-100">
                                    {move || format!("已删除的作品（{}）", deleted_novels.get().len())}
                                </h4>
                                {move || {
                                    let list = deleted_novels.get();
                                    if list.is_empty() {
                                        return view! {
                                            <div class="text-[11px] text-slate-400 py-2">
                                                "暂无已删除的作品"
                                            </div>
                                        }.into_any();
                                    }
                                    view! {
                                        <div class="space-y-1.5">
                                            <For
                                                each=move || deleted_novels.get()
                                                key=|t| t.id.clone()
                                                children=move |item| {
                                                    let rid = item.id.clone();
                                                    view! {
                                                        <div class="flex items-center justify-between p-2 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/60 opacity-80">
                                                            <div class="min-w-0">
                                                                <div class="font-semibold text-slate-600 dark:text-slate-300 truncate line-through">
                                                                    {item.title.clone()}
                                                                </div>
                                                                <div class="text-[10px] text-slate-400">
                                                                    "删除于 " {item.deleted_at.clone()}
                                                                </div>
                                                            </div>
                                                            <button
                                                                class="px-2.5 py-1 bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 border border-emerald-300 dark:border-emerald-800 rounded transition-colors cursor-pointer shrink-0"
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

                // 删除确认
                {move || {
                    let Some((_, title)) = pending_delete.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    view! {
                        <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center z-60 p-6">
                            <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-md w-full border border-red-200 dark:border-red-800 overflow-hidden">
                                <div class="px-5 py-3 bg-red-50 dark:bg-red-950/40 border-b border-red-200 dark:border-red-800 flex items-center gap-2">
                                    <span class="text-lg">"⚠️"</span>
                                    <h3 class="font-bold text-sm text-red-700 dark:text-red-300">
                                        "确认删除作品"
                                    </h3>
                                </div>
                                <div class="p-5 space-y-3 text-xs text-slate-700 dark:text-slate-300">
                                    <p>
                                        "即将删除《" <strong>{title.clone()}</strong> "》。"
                                    </p>
                                    <p class="text-slate-500 dark:text-slate-400">
                                        "作品及其全部卷、章节、设定会一并移入废弃状态，内容仍保留在本地，可在本窗口的「已删除的作品」中恢复。"
                                    </p>
                                </div>
                                <div class="px-5 py-3 bg-slate-50 dark:bg-slate-900/60 border-t border-slate-200 dark:border-slate-700 flex items-center justify-end gap-2">
                                    <button
                                        class="px-3.5 py-1.5 bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded font-medium transition-colors cursor-pointer"
                                        on:click=move |_| pending_delete.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="px-3.5 py-1.5 bg-red-600 hover:bg-red-700 text-white rounded font-medium transition-colors cursor-pointer disabled:opacity-50"
                                        disabled=move || is_busy.get()
                                        on:click=on_confirm_delete
                                    >
                                        "确认删除"
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

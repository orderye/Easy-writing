use crate::components::config::app_settings::{read_setting, write_setting};
use crate::tauri_api::invoke_no_args;
use leptos::prelude::*;
use shared_types::RetentionPreview;

/// 远端备份保留策略面板。
///
/// 保留策略是本应用**唯一会主动删除远端数据**的机制，所以这个面板必须
/// 让作者在做决定之前看见后果：
///
/// - 四个数字输入说明「会保留什么」；
/// - 「预览将被清理的备份」按钮走的是后端 dry-run，只读不删，
///   把「按当前策略会删掉哪些文件」原样列出来。
///
/// 三个维度同时为 0 是非法配置（等于「把所有备份都删掉」），后端会拒绝，
/// 前端也在保存前拦一道并说明原因。
#[component]
pub fn RetentionPanel() -> impl IntoView {
    let keep_recent = RwSignal::new("20".to_string());
    let keep_daily = RwSignal::new("7".to_string());
    let keep_weekly = RwSignal::new("4".to_string());
    let snapshot_limit = RwSignal::new("50".to_string());
    let status = RwSignal::new(String::new());
    let preview = RwSignal::new(Option::<RetentionPreview>::None);
    let is_previewing = RwSignal::new(false);

    // 载入当前设置
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            if let Ok(Some(v)) = read_setting("backup_keep_recent").await {
                keep_recent.set(v);
            }
            if let Ok(Some(v)) = read_setting("backup_keep_daily").await {
                keep_daily.set(v);
            }
            if let Ok(Some(v)) = read_setting("backup_keep_weekly").await {
                keep_weekly.set(v);
            }
            if let Ok(Some(v)) = read_setting("revision_snapshot_limit").await {
                snapshot_limit.set(v);
            }
        });
    });

    let parse_count = |raw: String| -> Result<u32, String> {
        let n = raw.trim().parse::<u32>().map_err(|_| "请输入非负整数".to_string())?;
        Ok(n)
    };

    let on_save = move |_| {
        let recent = keep_recent.get();
        let daily = keep_daily.get();
        let weekly = keep_weekly.get();
        let limit = snapshot_limit.get();

        let parsed = (
            parse_count(recent),
            parse_count(daily),
            parse_count(weekly),
            parse_count(limit),
        );
        let (r, d, w, l) = match parsed {
            (Ok(r), Ok(d), Ok(w), Ok(l)) => (r, d, w, l),
            _ => {
                status.set("✕ 数值不合法：请填写非负整数。".to_string());
                return;
            }
        };

        if r == 0 && d == 0 && w == 0 {
            status.set(
                "✕ 三个保留维度不能同时为 0：那等于「把所有远端备份都删掉」。请至少保留一项。"
                    .to_string(),
            );
            return;
        }

        status.set("正在保存保留策略……".to_string());
        leptos::task::spawn_local(async move {
            let _ = write_setting("backup_keep_recent", r.to_string()).await;
            let _ = write_setting("backup_keep_daily", d.to_string()).await;
            let _ = write_setting("backup_keep_weekly", w.to_string()).await;
            let _ = write_setting("revision_snapshot_limit", l.to_string()).await;
            status.set("✓ 保留策略已保存，将在下一次备份时生效。".to_string());
        });
    };

    let on_preview = move |_| {
        is_previewing.set(true);
        preview.set(None);
        status.set("正在计算将被清理的备份……".to_string());
        leptos::task::spawn_local(async move {
            match invoke_no_args::<RetentionPreview>("webdav_preview_retention").await {
                Ok(p) => {
                    status.set(format!(
                        "预览完成：受管备份 {} 份，保留 {} 份，将清理 {} 份。",
                        p.managed_count,
                        p.keep_count,
                        p.to_delete.len()
                    ));
                    preview.set(Some(p));
                }
                Err(e) => {
                    status.set(format!("✕ 预览失败：{e}"));
                }
            }
            is_previewing.set(false);
        });
    };

    view! {
        <div class="space-y-3 text-xs text-slate-700 dark:text-slate-300">
            <div class="font-bold text-xs text-slate-800 dark:text-slate-100">
                "远端备份保留策略"
            </div>
            <div class="text-[11px] text-slate-400 dark:text-slate-500">
                "三个维度取并集：一份备份只要命中任意一维就会保留。清理只在新备份上传并校验成功之后发生，且只处理本应用创建的 backup_*.db.gz。"
            </div>

            <div class="grid grid-cols-2 gap-3">
                <div class="space-y-1">
                    <label class="font-medium text-slate-600 dark:text-slate-400">"保留最近 N 份"</label>
                    <input
                        type="number"
                        min="0"
                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500"
                        prop:value=move || keep_recent.get()
                        on:input=move |ev| keep_recent.set(event_target_value(&ev))
                    />
                </div>
                <div class="space-y-1">
                    <label class="font-medium text-slate-600 dark:text-slate-400">"每日保留 1 份，覆盖最近 N 天"</label>
                    <input
                        type="number"
                        min="0"
                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500"
                        prop:value=move || keep_daily.get()
                        on:input=move |ev| keep_daily.set(event_target_value(&ev))
                    />
                </div>
                <div class="space-y-1">
                    <label class="font-medium text-slate-600 dark:text-slate-400">"每周保留 1 份，覆盖最近 N 周"</label>
                    <input
                        type="number"
                        min="0"
                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500"
                        prop:value=move || keep_weekly.get()
                        on:input=move |ev| keep_weekly.set(event_target_value(&ev))
                    />
                </div>
                <div class="space-y-1">
                    <label class="font-medium text-slate-600 dark:text-slate-400">"每章自动快照上限"</label>
                    <input
                        type="number"
                        min="1"
                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500"
                        prop:value=move || snapshot_limit.get()
                        on:input=move |ev| snapshot_limit.set(event_target_value(&ev))
                    />
                </div>
            </div>

            <div class="flex items-center gap-2">
                <button
                    class="px-3.5 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium shadow-xs transition-colors cursor-pointer"
                    on:click=on_save
                >
                    "保存保留策略"
                </button>
                <button
                    class="px-3.5 py-1.5 bg-slate-100 hover:bg-slate-200 text-slate-700 border border-slate-300 dark:bg-slate-700 dark:text-slate-200 dark:border-slate-600 rounded font-medium transition-colors cursor-pointer disabled:opacity-50"
                    disabled=move || is_previewing.get()
                    on:click=on_preview
                >
                    {move || if is_previewing.get() { "正在预览……" } else { "预览将被清理的备份" }}
                </button>
            </div>

            {move || if !status.get().is_empty() {
                view! {
                    <div class="p-2.5 bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800 rounded-md text-xs font-semibold text-blue-800 dark:text-blue-300 whitespace-pre-line">
                        {status.get()}
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}

            // 预览结果：把「会删掉哪些」逐条列出，不折叠成一句计数
            {move || {
                let Some(p) = preview.get() else {
                    return view! { <div></div> }.into_any();
                };
                let to_delete = p.to_delete.clone();
                let protected_foreign = p.protected_foreign;
                let policy = p.policy;
                view! {
                    <div class="space-y-2 p-2.5 rounded-md border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/60">
                        <div class="font-semibold text-slate-700 dark:text-slate-300">
                            "当前策略：最近 " {policy.recent} " 份 + 每日 " {policy.daily} " 天 + 每周 " {policy.weekly} " 周"
                        </div>
                        <div class="text-[11px] text-slate-500 dark:text-slate-400">
                            "远端另有 " {protected_foreign} " 个文件不属于本应用的备份命名范围，本策略永远不会删除它们。"
                        </div>
                        {move || if to_delete.is_empty() {
                            view! {
                                <div class="text-[11px] text-emerald-700 dark:text-emerald-300">
                                    "按当前策略，不会有任何备份被清理。"
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="space-y-1">
                                    <div class="text-[11px] font-semibold text-amber-800 dark:text-amber-300">
                                        "将被清理（" {to_delete.len()} " 份）："
                                    </div>
                                    <div class="max-h-32 overflow-y-auto space-y-0.5">
                                        {to_delete.iter().map(|n| view! {
                                            <div class="text-[11px] font-mono text-slate-600 dark:text-slate-400 break-all">
                                                {n.clone()}
                                            </div>
                                        }).collect::<Vec<_>>()}
                                    </div>
                                </div>
                            }.into_any()
                        }}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

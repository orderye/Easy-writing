use crate::state::AppContext;
use crate::tauri_api::invoke_cmd;
use leptos::prelude::*;

#[component]
pub fn UpdateModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let update_info = move || app_ctx.available_update.get();

    let on_open_download = move |url: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("open_external_url")
                .arg("url", url)
                .send_unit()
                .await;
        });
    };

    let on_ignore_version = move |version: String| {
        is_open.set(false);
        app_ctx.has_update_badge.set(false);
        leptos::task::spawn_local(async move {
            let _ = invoke_cmd("set_ignored_update_version")
                .arg("version", version)
                .send_unit()
                .await;
        });
    };

    view! {
        {move || {
            if is_open.get() {
                if let Some(info) = update_info() {
                    let info_clone = info.clone();
                    let target_download_url = info.download_url.clone();
                    let html_url = info.html_url.clone();
                    let version_for_ignore = info.latest_version.clone();
                    let assets = info.assets.clone();

                    view! {
                        <div class="fixed inset-0 bg-slate-900/60 backdrop-blur-xs flex items-center justify-center z-50 p-4 select-none">
                            <div class="bg-white dark:bg-slate-800 rounded-2xl shadow-2xl max-w-xl w-full border border-slate-200 dark:border-slate-700 overflow-hidden flex flex-col max-h-[85vh] animate-in fade-in zoom-in-95">
                                // Header Banner
                                <div class="px-6 py-5 bg-gradient-to-r from-blue-600 via-indigo-600 to-purple-600 text-white flex items-center justify-between">
                                    <div class="flex items-center gap-3">
                                        <div class="w-10 h-10 rounded-xl bg-white/20 backdrop-blur-md flex items-center justify-center text-xl shadow-inner">
                                            "✨"
                                        </div>
                                        <div>
                                            <h3 class="font-bold text-base tracking-tight leading-snug">"发现 Easy-writing 新版本"</h3>
                                            <p class="text-xs text-blue-100 font-medium">
                                                {format!("当前版本 v{} · 最新发布 v{}", info.current_version, info.latest_version)}
                                            </p>
                                        </div>
                                    </div>
                                    <button
                                        class="text-white/80 hover:text-white hover:bg-white/10 rounded-lg p-1.5 transition-colors cursor-pointer text-base"
                                        title="关闭"
                                        on:click=move |_| is_open.set(false)
                                    >
                                        "✕"
                                    </button>
                                </div>

                                // Content Body
                                <div class="p-6 space-y-4 overflow-y-auto flex-1 text-slate-700 dark:text-slate-300 text-xs">
                                    // Release Title & Date Badge
                                    <div class="flex items-center justify-between border-b border-slate-100 dark:border-slate-700 pb-3">
                                        <div class="font-bold text-sm text-slate-800 dark:text-slate-100">
                                            {info_clone.release_name}
                                        </div>
                                        <span class="text-[11px] font-mono bg-blue-50 dark:bg-blue-950/60 text-blue-600 dark:text-blue-400 px-2.5 py-0.5 rounded-full border border-blue-200 dark:border-blue-800">
                                            {if !info_clone.published_at.is_empty() {
                                                info_clone.published_at.chars().take(10).collect::<String>()
                                            } else {
                                                "最新发布".to_string()
                                            }}
                                        </span>
                                    </div>

                                    // Release Notes
                                    <div class="space-y-1.5">
                                        <label class="font-semibold text-slate-600 dark:text-slate-400 text-[11px] uppercase tracking-wider block">
                                            "版本更新日志"
                                        </label>
                                        <div class="bg-slate-50 dark:bg-slate-900/60 rounded-xl p-3.5 border border-slate-200 dark:border-slate-700/70 max-h-56 overflow-y-auto font-sans leading-relaxed text-slate-700 dark:text-slate-300 whitespace-pre-wrap select-text">
                                            {info_clone.release_notes}
                                        </div>
                                    </div>

                                    // Direct assets if available
                                    {if !assets.is_empty() {
                                        view! {
                                            <div class="space-y-1.5">
                                                <label class="font-semibold text-slate-600 dark:text-slate-400 text-[11px] uppercase tracking-wider block">
                                                    "安装包文件下载"
                                                </label>
                                                <div class="grid grid-cols-1 gap-1.5">
                                                    {assets.into_iter().map(|asset| {
                                                        let asset_url = asset.browser_download_url.clone();
                                                        let size_mb = format!("{:.1} MB", (asset.size as f64) / (1024.0 * 1024.0));
                                                        let sha = asset.sha256.clone();
                                                        view! {
                                                            <div
                                                                class="flex items-center justify-between p-2 rounded-lg bg-slate-50 dark:bg-slate-900/40 hover:bg-blue-50 dark:hover:bg-blue-950/40 border border-slate-200 dark:border-slate-700/60 cursor-pointer transition-colors group"
                                                                on:click=move |_| on_open_download(asset_url.clone())
                                                            >
                                                                <div class="flex items-center gap-2 truncate">
                                                                    <span class="text-slate-400 group-hover:text-blue-500">"📦"</span>
                                                                    <span class="font-mono text-slate-800 dark:text-slate-200 font-medium truncate">{asset.name}</span>
                                                                </div>
                                                                <div class="flex items-center gap-2 shrink-0">
                                                                    <span class="text-slate-400 font-mono text-[10px]">{size_mb}</span>
                                                                    <span class="text-blue-600 dark:text-blue-400 font-semibold group-hover:underline">"下载 →"</span>
                                                                </div>
                                                            </div>
                                                            // SHA-256 展示（任务书 T13）：来源是 Release 的 SHA256SUMS.txt，
                                                            // 后端没匹配到就为空 —— 空值不展示，不编造。
                                                            {if !sha.is_empty() {
                                                                view! {
                                                                    <div class="px-2 pb-1.5 -mt-1 space-y-0.5">
                                                                        <div class="flex items-center gap-1.5 text-[10px] font-mono text-slate-500 dark:text-slate-400 select-text">
                                                                            <span class="shrink-0 font-sans font-semibold text-slate-400 dark:text-slate-500">"SHA-256"</span>
                                                                            <span class="truncate" title={sha.clone()}>{sha.clone()}</span>
                                                                        </div>
                                                                        <div class="text-[10px] text-slate-400 dark:text-slate-500 font-sans">
                                                                            "校验方法：下载后执行 shasum -a 256 <文件名>（macOS）或 certutil -hashfile <文件名> SHA256（Windows），与上行比对一致方可安装。"
                                                                        </div>
                                                                    </div>
                                                                }.into_any()
                                                            } else {
                                                                view! { <div></div> }.into_any()
                                                            }}
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <div></div> }.into_any()
                                    }}
                                </div>

                                // Footer Actions
                                <div class="px-6 py-4 bg-slate-50 dark:bg-slate-800/80 border-t border-slate-200 dark:border-slate-700 flex items-center justify-between gap-2">
                                    <button
                                        class="text-xs text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 transition-colors cursor-pointer"
                                        title="不再提示此版本的更新"
                                        on:click=move |_| on_ignore_version(version_for_ignore.clone())
                                    >
                                        "跳过此版本"
                                    </button>

                                    <div class="flex items-center gap-2.5">
                                        <button
                                            class="px-3.5 py-1.5 rounded-lg border border-slate-300 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 font-medium text-xs transition-colors cursor-pointer"
                                            on:click=move |_| is_open.set(false)
                                        >
                                            "稍后提醒"
                                        </button>
                                        <button
                                            class="px-4 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-700 active:bg-blue-800 text-white font-medium text-xs shadow-sm hover:shadow transition-all flex items-center gap-1.5 cursor-pointer"
                                            on:click=move |_| on_open_download(if !target_download_url.is_empty() { target_download_url.clone() } else { html_url.clone() })
                                        >
                                            <span>"🚀 前往下载 / 发布页"</span>
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            } else {
                view! { <div></div> }.into_any()
            }
        }}
    }
}

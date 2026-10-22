use crate::components::config::retention_panel::RetentionPanel;
use crate::tauri_api::{invoke_cmd, invoke_no_args};
use leptos::prelude::*;
use shared_types::{
    BackupFileInfo, BackupReport, ManifestCheck, RestoreOutcome, RestorePreflight, RestoreReport,
    WebdavConfig,
};

/// 清单校验状态的人话版本。
///
/// 「无清单」与「清单校验通过」必须分开展示：前者意味着这份备份只能按
/// 结构完整性来判断，作者需要知道少了一层保障。
fn manifest_label(m: &ManifestCheck) -> String {
    match m {
        ManifestCheck::Absent => "无清单（历史备份）· 仅通过 SQLite 结构完整性校验".to_string(),
        ManifestCheck::Verified { asset_count } => {
            format!("清单校验通过 · 含 {asset_count} 个素材记录")
        }
    }
}

fn outcome_label(o: &RestoreOutcome) -> &'static str {
    match o {
        RestoreOutcome::Success => "恢复完成",
        RestoreOutcome::PartialAssets => "部分恢复：数据库已恢复，但有素材缺失",
        RestoreOutcome::RolledBack => "恢复失败，已回滚：原数据完好",
        RestoreOutcome::RollbackFailed => "恢复失败且回滚失败：需要人工介入",
    }
}

/// 四种结局用四种视觉等级。把「回滚失败」和「完全成功」渲染成同一个样子，
/// 等于让最需要作者立刻行动的情况被淹没在一行绿字里。
fn outcome_classes(o: &RestoreOutcome) -> &'static str {
    match o {
        RestoreOutcome::Success => {
            "p-3 rounded-md border bg-emerald-50 dark:bg-emerald-950/40 border-emerald-300 dark:border-emerald-700"
        }
        RestoreOutcome::PartialAssets => {
            "p-3 rounded-md border bg-amber-50 dark:bg-amber-950/40 border-amber-300 dark:border-amber-700"
        }
        RestoreOutcome::RolledBack => {
            "p-3 rounded-md border bg-orange-50 dark:bg-orange-950/40 border-orange-300 dark:border-orange-700"
        }
        RestoreOutcome::RollbackFailed => {
            "p-3 rounded-md border-2 bg-red-50 dark:bg-red-950/50 border-red-500 dark:border-red-600"
        }
    }
}

fn outcome_text_classes(o: &RestoreOutcome) -> &'static str {
    match o {
        RestoreOutcome::Success => "text-emerald-800 dark:text-emerald-200",
        RestoreOutcome::PartialAssets => "text-amber-800 dark:text-amber-200",
        RestoreOutcome::RolledBack => "text-orange-800 dark:text-orange-200",
        RestoreOutcome::RollbackFailed => "text-red-800 dark:text-red-200",
    }
}

/// 「WebDAV 整库同步」标签页。
///
/// 从原 config_modal.rs 拆出：凭据表单、云端快照列表与备份/还原动作
/// 只服务于本标签页，拆开后这些状态不再与其他标签页共享。
#[component]
pub fn WebdavTab() -> impl IntoView {
    let webdav_url = RwSignal::new("https://dav.jianguoyun.com/dav/".to_string());
    let webdav_user = RwSignal::new(String::new());
    let webdav_pwd = RwSignal::new(String::new());
    let webdav_dir = RwSignal::new("/EasyWritingBackup".to_string());
    let webdav_status = RwSignal::new(String::new());
    let is_webdav_syncing = RwSignal::new(false);
    let backups_list = RwSignal::new(Vec::<BackupFileInfo>::new());
    let backup_result = RwSignal::new(Option::<BackupReport>::None);
    let restore_result = RwSignal::new(Option::<RestoreReport>::None);
    // 预检通过、等待作者确认的候选备份。Some(..) 即显示确认弹窗。
    let pending_restore = RwSignal::new(Option::<RestorePreflight>::None);

    // 挂载时载入已保存的凭据与云端快照列表
    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            if let Ok(Some(cfg)) = invoke_no_args::<Option<WebdavConfig>>("webdav_get_config").await
            {
                webdav_url.set(cfg.url);
                webdav_user.set(cfg.username);
                webdav_dir.set(cfg.remote_dir);
            }
            if let Ok(list) = invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await {
                backups_list.set(list);
            }
        });
    });

    let refresh_backups = move || {
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await {
                backups_list.set(list);
            }
        });
    };

    let on_save_webdav = move |_| {
        webdav_status.set("正在保存并测试 WebDAV 连通性...".to_string());
        let pwd_val = webdav_pwd.get();
        let pwd_opt = if pwd_val.trim().is_empty() {
            None
        } else {
            Some(pwd_val)
        };

        leptos::task::spawn_local(async move {
            let save_res = invoke_cmd("webdav_save_config")
                .arg("url", webdav_url.get())
                .arg("username", webdav_user.get())
                .opt("password", pwd_opt)
                .arg("remote_dir", webdav_dir.get())
                .send_unit()
                .await;

            if let Err(e) = save_res {
                webdav_status.set(format!("✕ 保存配置失败: {}", e));
                return;
            }

            let test_res = invoke_no_args::<()>("webdav_test_connection").await;
            match test_res {
                Ok(_) => {
                    webdav_status
                        .set("✓ WebDAV 连接成功！配置已安全保存至系统钥匙串。".to_string());
                    if let Ok(list) =
                        invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await
                    {
                        backups_list.set(list);
                    }
                }
                Err(err) => {
                    webdav_status.set(format!("✕ WebDAV 连接测试失败: {}", err));
                }
            }
        });
    };

    let on_backup_now = move |_| {
        is_webdav_syncing.set(true);
        backup_result.set(None);
        webdav_status.set("正在执行整库一致性快照与图片增量备份……".to_string());

        leptos::task::spawn_local(async move {
            let res = invoke_no_args::<BackupReport>("webdav_backup_now").await;
            match res {
                Ok(report) => {
                    // 「✓ 备份成功」不足以说明问题：清单没传上去、图片没同步、
                    // 素材本地就缺，这些都必须让作者当场看见。
                    webdav_status.set(if report.has_gaps() {
                        format!(
                            "⚠ 备份已完成但存在完整性缺口：{}",
                            report.filename
                        )
                    } else {
                        format!("✓ 备份成功！云端快照包: {}", report.filename)
                    });
                    // 清理了多少旧备份同样要可见 —— 保留策略是本应用唯一会
                    // 主动删除远端数据的机制，删了多少必须让作者知道。
                    if report.rotated_deleted > 0 {
                        webdav_status.set(format!(
                            "{}（已按保留策略清理 {} 份旧备份）",
                            webdav_status.get(),
                            report.rotated_deleted
                        ));
                    }
                    backup_result.set(Some(report));
                    if let Ok(list) =
                        invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await
                    {
                        backups_list.set(list);
                    }
                }
                Err(e) => {
                    webdav_status.set(format!("✕ 备份失败: {}", e));
                }
            }
            is_webdav_syncing.set(false);
        });
    };

    // 第一步：预检。**不改动数据库**，只把候选备份的事实摆出来。
    let on_inspect_backup = move |filename: String| {
        let fn_clone = filename.clone();
        is_webdav_syncing.set(true);
        restore_result.set(None);
        pending_restore.set(None);
        webdav_status.set(format!("正在下载并校验 {}……", fn_clone));

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("webdav_restore_preflight")
                .arg("filename", fn_clone.clone())
                .send::<RestorePreflight>()
                .await;

            match res {
                Ok(pf) => {
                    webdav_status.set(format!(
                        "✓ {} 预检通过，请确认下方内容后再恢复。",
                        fn_clone
                    ));
                    pending_restore.set(Some(pf));
                }
                Err(e) => {
                    // 预检失败时后端保证原数据未被改动，这句话必须转达给作者。
                    webdav_status.set(format!("✕ 预检未通过，当前数据未被改动：{}", e));
                }
            }
            is_webdav_syncing.set(false);
        });
    };

    // 第二步：作者确认后才真正覆盖数据库。
    let on_confirm_restore = move |pf: RestorePreflight| {
        let filename = pf.filename.clone();
        let token = pf.preflight_token.clone();
        let confirm_empty = pf.requires_empty_overwrite_confirm;
        pending_restore.set(None);
        is_webdav_syncing.set(true);
        webdav_status.set(format!("正在恢复 {}……", filename));

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("webdav_restore_backup")
                .arg("filename", filename.clone())
                .arg("preflight_token", token)
                .arg("confirm_empty_overwrite", confirm_empty)
                .send::<RestoreReport>()
                .await;

            match res {
                Ok(report) => {
                    webdav_status.set(format!(
                        "{}（{}）",
                        match report.outcome {
                            RestoreOutcome::Success => "✓",
                            RestoreOutcome::PartialAssets => "⚠",
                            RestoreOutcome::RolledBack => "✕",
                            RestoreOutcome::RollbackFailed => "✕",
                        },
                        outcome_label(&report.outcome)
                    ));
                    restore_result.set(Some(report));
                }
                Err(e) => {
                    webdav_status.set(format!("✕ 还原未执行：{}", e));
                }
            }
            is_webdav_syncing.set(false);
        });
    };

    // 供外部（保存/备份后）触发刷新；此处保留闭包以便后续扩展
    let _ = refresh_backups;

    view! {
        <div class="space-y-6 text-xs text-slate-700 dark:text-slate-300">
            <div class="grid grid-cols-2 gap-6">
                // Form
                <div class="space-y-3">
                    <h4 class="font-bold text-xs text-slate-800 dark:text-slate-100">"WebDAV 服务器凭据"</h4>
                    <div class="space-y-1">
                        <label class="font-medium text-slate-600 dark:text-slate-400">"WebDAV 服务器地址"</label>
                        <input
                            type="text"
                            class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 font-mono text-xs focus:outline-blue-500"
                            placeholder="https://dav.jianguoyun.com/dav/"
                            prop:value=move || webdav_url.get()
                            on:input=move |ev| webdav_url.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="space-y-1">
                        <label class="font-medium text-slate-600 dark:text-slate-400">"用户名"</label>
                        <input
                            type="text"
                            class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500"
                            placeholder="user@example.com"
                            prop:value=move || webdav_user.get()
                            on:input=move |ev| webdav_user.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="space-y-1">
                        <label class="font-medium text-slate-600 dark:text-slate-400">"应用专用密码 / Token（保存至系统钥匙串）"</label>
                        <input
                            type="password"
                            class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500 font-mono"
                            placeholder="••••••••••••"
                            prop:value=move || webdav_pwd.get()
                            on:input=move |ev| webdav_pwd.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="space-y-1">
                        <label class="font-medium text-slate-600 dark:text-slate-400">"远程备份目录"</label>
                        <input
                            type="text"
                            class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-xs focus:outline-blue-500 font-mono"
                            placeholder="/EasyWritingBackup"
                            prop:value=move || webdav_dir.get()
                            on:input=move |ev| webdav_dir.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="flex items-center gap-2 pt-1">
                        <button
                            class="px-3.5 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium shadow-xs transition-colors cursor-pointer"
                            on:click=on_save_webdav
                        >
                            "保存并测试连通性"
                        </button>
                        <button
                            class="px-3.5 py-1.5 bg-emerald-600 hover:bg-emerald-700 text-white rounded font-medium shadow-xs transition-colors cursor-pointer disabled:opacity-50"
                            disabled=move || is_webdav_syncing.get()
                            on:click=on_backup_now
                        >
                            {move || if is_webdav_syncing.get() { "正在备份..." } else { "立即整库备份" }}
                        </button>
                    </div>

                    // 备份完整性缺口：清单没传上去 / 图片没同步 / 素材本地就缺
                    {move || {
                        let Some(r) = backup_result.get() else {
                            return view! { <div></div> }.into_any();
                        };
                        if !r.has_gaps() {
                            return view! { <div></div> }.into_any();
                        }
                        let mut lines: Vec<String> = Vec::new();
                        if !r.verified {
                            lines.push("备份包未通过大小校验，本次不会轮转旧备份。".to_string());
                        }
                        if !r.manifest_uploaded {
                            lines.push("清单上传失败：这份备份只能按「历史无清单备份」恢复，少一层完整性保障。".to_string());
                        }
                        if !r.image_upload_failures.is_empty() {
                            lines.push(format!(
                                "{} 张图片上传失败：{}",
                                r.image_upload_failures.len(),
                                r.image_upload_failures.join("、")
                            ));
                        }
                        if !r.missing_assets.is_empty() {
                            lines.push(format!(
                                "{} 个素材在本地已缺失，未进入本次备份：{}",
                                r.missing_assets.len(),
                                r.missing_assets.join("、")
                            ));
                        }
                        view! {
                            <div class="p-2.5 rounded-md border border-amber-300 dark:border-amber-700 bg-amber-50 dark:bg-amber-950/40 text-amber-900 dark:text-amber-200 space-y-1">
                                <div class="font-bold">"备份存在完整性缺口"</div>
                                {lines.into_iter().map(|l| view! { <div>{l}</div> }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}

                    // 保留策略：唯一会主动删除远端数据的机制，放在备份按钮旁边
                    <div class="pt-2 border-t border-slate-200 dark:border-slate-700">
                        <RetentionPanel />
                    </div>
                </div>

                // Cloud Backups List
                <div class="space-y-3">
                    <h4 class="font-bold text-xs text-slate-800 dark:text-slate-100">"云端备份快照历史（恢复前会先预检并请你确认）"</h4>
                    <div class="border border-slate-200 dark:border-slate-700 rounded-lg p-3 bg-slate-50 dark:bg-slate-900/60 min-h-[220px] max-h-[320px] overflow-y-auto space-y-2">
                        {move || {
                            let list = backups_list.get();
                            if list.is_empty() {
                                view! {
                                    <div class="text-center py-10 text-slate-400 text-xs">
                                        "暂无云端备份文件，或未成功连接 WebDAV。"
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <For
                                        each=move || backups_list.get()
                                        key=|b| b.filename.clone()
                                        children=move |b| {
                                            let fn_for_restore = b.filename.clone();
                                            let kb = b.size_bytes / 1024;
                                            view! {
                                                <div class="flex items-center justify-between p-2 bg-white dark:bg-slate-800 rounded border border-slate-200 dark:border-slate-700 text-xs shadow-2xs">
                                                    <div>
                                                        <div class="font-mono font-semibold text-slate-800 dark:text-slate-100 text-[11px]">
                                                            {b.filename.clone()}
                                                        </div>
                                                        <div class="text-[10px] text-slate-400">
                                                            {kb} " KB · " {b.modified_at.clone()}
                                                        </div>
                                                    </div>
                                                    <button
                                                        class="px-2 py-1 bg-amber-50 hover:bg-amber-100 text-amber-700 border border-amber-300 rounded text-[11px] font-medium cursor-pointer"
                                                        title="先预检这份备份，确认后再恢复"
                                                        on:click=move |_| on_inspect_backup(fn_for_restore.clone())
                                                    >
                                                        "预检并还原"
                                                    </button>
                                                </div>
                                            }
                                        }
                                    />
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
            </div>

            {move || if !webdav_status.get().is_empty() {
                view! {
                    <div class="p-2.5 bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800 rounded-md text-xs font-semibold text-blue-800 dark:text-blue-300 whitespace-pre-line">
                        {webdav_status.get()}
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}

            // 恢复结果：四类结局分开渲染，绝不折叠成一行「还原成功」
            {move || {
                let Some(r) = restore_result.get() else {
                    return view! { <div></div> }.into_any();
                };
                let s = r.summary.clone();
                let outcome = r.outcome.clone();
                let box_cls = outcome_classes(&outcome);
                let text_cls = outcome_text_classes(&outcome);
                let dangling: Vec<String> = r.dangling_keys.iter().map(|k| k.profile_name.clone()).collect();
                let issues = r.issues.clone();

                view! {
                    <div class=box_cls>
                        <div class="space-y-2 text-xs">
                            <div class=format!("font-bold {}", text_cls)>{outcome_label(&outcome)}</div>

                            <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-slate-700 dark:text-slate-300">
                                <div>"数据库恢复："{if s.database_restored { "成功" } else { "未执行 / 已回滚" }}</div>
                                <div>"本地配置写回："{if s.config_restored { "成功" } else { "失败或未执行" }}</div>
                                <div>"清单校验："{manifest_label(&s.manifest)}</div>
                                <div>"SQLite 完整性："{if s.database_restored { "已通过" } else { "见下方说明" }}</div>
                                <div>{format!("备份 schema：v{}", s.schema_version)}</div>
                                <div>"素材："{s.asset_restored} " / " {s.asset_total} " 已就位，" {s.asset_failed} " 失败"</div>
                                <div>"作品 / 卷 / 章：" {s.novel_count} " / " {s.volume_count} " / " {s.chapter_count}</div>
                                <div>"总字数："{s.word_count}</div>
                            </div>

                            {move || if s.is_empty_library {
                                view! {
                                    <div class="p-2 rounded border border-amber-400 dark:border-amber-600 bg-amber-100/60 dark:bg-amber-950/50 text-amber-900 dark:text-amber-200 font-semibold">
                                        "这份备份是空库：恢复后作品列表为空。若这不是你的本意，请立即用安全快照回退。"
                                    </div>
                                }.into_any()
                            } else { view! { <div></div> }.into_any() }}

                            {move || if !s.config_error.is_empty() {
                                view! {
                                    <div class="text-red-700 dark:text-red-300">
                                        "配置阶段错误："{s.config_error.clone()}
                                    </div>
                                }.into_any()
                            } else { view! { <div></div> }.into_any() }}

                            {move || if !s.rollback_error.is_empty() {
                                view! {
                                    <div class="text-red-700 dark:text-red-300 font-semibold">
                                        "回滚错误："{s.rollback_error.clone()}
                                    </div>
                                }.into_any()
                            } else { view! { <div></div> }.into_any() }}

                            {move || if s.snapshot_retained && !s.snapshot_path.is_empty() {
                                view! {
                                    <div class="text-slate-600 dark:text-slate-400 font-mono text-[10px] break-all">
                                        "安全快照已保留（可用于人工回退）："{s.snapshot_path.clone()}
                                    </div>
                                }.into_any()
                            } else { view! { <div></div> }.into_any() }}

                            {move || if !dangling.is_empty() {
                                view! {
                                    <div class="text-amber-800 dark:text-amber-200">
                                        "⚠ 以下模型配置在本机缺少钥匙串密钥，请到「模型接口」重新填入："
                                        {dangling.join("、")}
                                    </div>
                                }.into_any()
                            } else { view! { <div></div> }.into_any() }}

                            {move || if issues.is_empty() {
                                view! { <div></div> }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-1 border-t border-slate-300/60 dark:border-slate-600/60 pt-2">
                                        <div class="font-semibold text-slate-700 dark:text-slate-300">
                                            "明细（"{issues.len()}" 项）"
                                        </div>
                                        <div class="max-h-40 overflow-y-auto space-y-1">
                                            {issues.iter().map(|i| view! {
                                                <div class="text-[11px] text-slate-600 dark:text-slate-400">
                                                    <span class="font-mono">{i.filename.clone()}</span>
                                                    " — "{i.reason.clone()}
                                                </div>
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    </div>
                }.into_any()
            }}

            // 恢复确认弹窗：**必须在覆盖数据库之前**出现
            {move || {
                let Some(pf) = pending_restore.get() else {
                    return view! { <div></div> }.into_any();
                };
                let dangerous = pf.requires_empty_overwrite_confirm;
                let header_cls = if dangerous {
                    "px-5 py-3 bg-red-50 dark:bg-red-950/40 border-b border-red-200 dark:border-red-800 flex items-center gap-2"
                } else {
                    "px-5 py-3 bg-blue-50 dark:bg-blue-950/40 border-b border-blue-200 dark:border-blue-800 flex items-center gap-2"
                };
                let confirm_cls = if dangerous {
                    "px-3.5 py-1.5 bg-red-600 hover:bg-red-700 text-white rounded font-medium transition-colors cursor-pointer"
                } else {
                    "px-3.5 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium transition-colors cursor-pointer"
                };
                let confirm_label = if dangerous {
                    format!("确认用空库替换现有 {} 部作品", pf.local_novel_count)
                } else {
                    "确认恢复这份备份".to_string()
                };
                let title = if dangerous { "将用空库替换当前作品数据" } else { "确认恢复" };
                let problems = pf.asset_problems.clone();
                let pf_for_confirm = pf.clone();

                view! {
                    <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-sm flex items-center justify-center z-[60] p-6">
                        <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-lg w-full border border-slate-200 dark:border-slate-700 overflow-hidden">
                            <div class=header_cls>
                                <span class="text-lg">{if dangerous { "⚠️" } else { "ℹ️" }}</span>
                                <h3 class="font-bold text-sm text-slate-800 dark:text-slate-100">{title}</h3>
                            </div>
                            <div class="p-5 space-y-3 text-xs text-slate-700 dark:text-slate-300 max-h-[60vh] overflow-y-auto">
                                <div class="font-mono text-[11px] text-slate-500 dark:text-slate-400 break-all">
                                    {pf.filename.clone()}
                                </div>

                                {move || if dangerous {
                                    view! {
                                        <div class="p-2.5 rounded border-2 border-red-400 dark:border-red-600 bg-red-50 dark:bg-red-950/50 text-red-800 dark:text-red-200 font-semibold">
                                            "这份备份里一部作品都没有，而你本机当前有 "
                                            {pf.local_novel_count}" 部作品。"
                                            "继续将用空库替换当前全部已有作品数据，此操作只能通过本地安全快照回退。"
                                        </div>
                                    }.into_any()
                                } else { view! { <div></div> }.into_any() }}

                                <div class="grid grid-cols-2 gap-x-4 gap-y-1">
                                    <div>{format!("备份 schema：v{}", pf.schema_version)}</div>
                                    <div>"SQLite 完整性："{if pf.integrity_ok { "已通过" } else { "未通过" }}</div>
                                    <div>"作品 / 卷 / 章：" {pf.novel_count} " / " {pf.volume_count} " / " {pf.chapter_count}</div>
                                    <div>"总字数："{pf.word_count}</div>
                                    <div>"本机当前作品数："{pf.local_novel_count}</div>
                                    <div>"是否空库："{if pf.is_empty_library { "是" } else { "否" }}</div>
                                </div>

                                <div class="p-2 rounded bg-slate-50 dark:bg-slate-900/60 border border-slate-200 dark:border-slate-700">
                                    <div class="font-semibold mb-1">"清单与素材"</div>
                                    <div>{manifest_label(&pf.manifest)}</div>
                                    <div>
                                        "素材：" {pf.asset_present_locally} " 个本机已就位，"
                                        {pf.asset_missing_locally} " 个需从云端回填（共 " {pf.asset_total} " 个）"
                                    </div>
                                </div>

                                {move || if problems.is_empty() {
                                    view! { <div></div> }.into_any()
                                } else {
                                    view! {
                                        <div class="p-2 rounded bg-amber-50 dark:bg-amber-950/40 border border-amber-300 dark:border-amber-700 space-y-1">
                                            <div class="font-semibold text-amber-900 dark:text-amber-200">"需要注意的问题"</div>
                                            {problems.iter().map(|p| view! {
                                                <div class="text-[11px] text-amber-800 dark:text-amber-300">{"· "}{p.clone()}</div>
                                            }).collect::<Vec<_>>()}
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                            <div class="px-5 py-3 bg-slate-50 dark:bg-slate-900/60 border-t border-slate-200 dark:border-slate-700 flex justify-end gap-2">
                                <button
                                    class="px-3.5 py-1.5 bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 rounded font-medium transition-colors cursor-pointer"
                                    on:click=move |_| {
                                        pending_restore.set(None);
                                        webdav_status.set("已取消恢复，当前数据未做任何改动。".to_string());
                                    }
                                >
                                    "取消"
                                </button>
                                <button
                                    class=confirm_cls
                                    on:click=move |_| on_confirm_restore(pf_for_confirm.clone())
                                >
                                    {confirm_label}
                                </button>
                            </div>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

use crate::tauri_api::{invoke_cmd, invoke_no_args};
use leptos::prelude::*;
use shared_types::{BackupFileInfo, WebdavConfig};

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
        webdav_status.set("正在执行整库一致性快照与图片增量备份……".to_string());

        leptos::task::spawn_local(async move {
            let res = invoke_no_args::<String>("webdav_backup_now").await;
            match res {
                Ok(filename) => {
                    webdav_status.set(format!("✓ 备份成功！云端快照包: {}", filename));
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

    let on_restore_backup = move |filename: String| {
        let fn_clone = filename.clone();
        is_webdav_syncing.set(true);
        webdav_status.set(format!(
            "正在还原版本 {}，并自动在本地留存安全快照……",
            fn_clone
        ));

        leptos::task::spawn_local(async move {
            let res = invoke_cmd("webdav_restore_backup")
                .arg("filename", fn_clone.clone())
                .send::<shared_types::RestoreReport>()
                .await;

            match res {
                Ok(report) => {
                    let mut msg = format!(
                        "✓ 版本 {} 还原成功！本地旧版已留存至 backups/ 目录防覆写。",
                        fn_clone
                    );
                    if !report.dangling_keys.is_empty() {
                        let names: Vec<String> = report.dangling_keys.into_iter().map(|k| k.profile_name).collect();
                        msg.push_str(&format!(
                            " ⚠️ 检测到以下模型配置缺少本机钥匙串密钥，请在「模型接口」中重新填入：{}",
                            names.join("、")
                        ));
                    }
                    if !report.issues.is_empty() {
                        msg.push_str(&format!(" (另有 {} 项素材补齐异常)", report.issues.len()));
                    }
                    webdav_status.set(msg);
                }
                Err(e) => {
                    webdav_status.set(format!("✕ 还原失败: {}", e));
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
                </div>

                // Cloud Backups List
                <div class="space-y-3">
                    <h4 class="font-bold text-xs text-slate-800 dark:text-slate-100">"云端备份快照历史（带防覆写恢复）"</h4>
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
                                                        title="安全还原此版本"
                                                        on:click=move |_| on_restore_backup(fn_for_restore.clone())
                                                    >
                                                        "还原"
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
                    <div class="p-2.5 bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800 rounded-md text-xs font-semibold text-blue-800 dark:text-blue-300">
                        {webdav_status.get()}
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }}
        </div>
    }
}

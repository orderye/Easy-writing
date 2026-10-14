use crate::state::AppContext;
use crate::tauri_api::{invoke, invoke_no_args};
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{Agent, BackupFileInfo, LlmProfile, WebdavConfig};

#[derive(Serialize)]
struct CreateProfileArgs {
    name: String,
    protocol: String,
    base_url: String,
    model: String,
    api_key: String,
    is_default: bool,
}

#[derive(Serialize)]
struct DeleteProfileArgs {
    id: String,
}

#[derive(Serialize)]
struct TestConnectionArgs {
    profile_id: String,
}

#[allow(dead_code)]
#[derive(Serialize)]
struct UpdateAgentArgs {
    key: String,
    system_prompt: String,
    llm_profile_id: Option<String>,
    enabled: bool,
}

#[derive(Serialize)]
struct SaveWebdavArgs {
    url: String,
    username: String,
    password: Option<String>,
    remote_dir: String,
}

#[derive(Serialize)]
struct RestoreBackupArgs {
    filename: String,
}

#[derive(Serialize)]
struct GetAppSettingArgs {
    key: String,
}

#[derive(Serialize)]
struct SetAppSettingArgs {
    key: String,
    value: String,
}

#[component]
pub fn ConfigModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let active_tab = RwSignal::new("llm".to_string()); // "llm" | "agent" | "webdav" | "pref"
    let profiles = RwSignal::new(Vec::<LlmProfile>::new());
    let agents = RwSignal::new(Vec::<Agent>::new());

    // New profile form state
    let form_name = RwSignal::new(String::new());
    let form_protocol = RwSignal::new("openai".to_string());
    let form_base_url = RwSignal::new("https://api.openai.com/v1".to_string());
    let form_model = RwSignal::new("gpt-4o".to_string());
    let form_api_key = RwSignal::new(String::new());

    let test_status = RwSignal::new(String::new());

    // WebDAV state
    let webdav_url = RwSignal::new("https://dav.jianguoyun.com/dav/".to_string());
    let webdav_user = RwSignal::new(String::new());
    let webdav_pwd = RwSignal::new(String::new());
    let webdav_dir = RwSignal::new("/EasyWritingBackup".to_string());
    let webdav_status = RwSignal::new(String::new());
    let is_webdav_syncing = RwSignal::new(false);
    let backups_list = RwSignal::new(Vec::<BackupFileInfo>::new());

    // App Preferences state
    let theme = RwSignal::new("system".to_string());
    let font_size = RwSignal::new("16".to_string());
    let line_height = RwSignal::new("1.8".to_string());
    let auto_save_interval = RwSignal::new("30".to_string());
    let pref_status = RwSignal::new(String::new());

    let load_data = move || {
        leptos::task::spawn_local(async move {
            if let Ok(list) = invoke_no_args::<Vec<LlmProfile>>("list_llm_profiles").await {
                profiles.set(list);
            }
            if let Ok(list) = invoke_no_args::<Vec<Agent>>("list_agents").await {
                agents.set(list);
            }

            // Load WebDAV Config
            if let Ok(Some(cfg)) = invoke_no_args::<Option<WebdavConfig>>("webdav_get_config").await {
                webdav_url.set(cfg.url);
                webdav_user.set(cfg.username);
                webdav_dir.set(cfg.remote_dir);
            }

            // Load WebDAV Backup List
            if let Ok(list) = invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await {
                backups_list.set(list);
            }

            // Load Preferences
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "theme".to_string() }).await {
                theme.set(val);
            }
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "font_size".to_string() }).await {
                font_size.set(val);
            }
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "line_height".to_string() }).await {
                line_height.set(val);
            }
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "auto_save_interval".to_string() }).await {
                auto_save_interval.set(val);
            }
        });
    };

    Effect::new(move |_| {
        if is_open.get() {
            load_data();
        }
    });

    let on_add_profile = move |_| {
        leptos::task::spawn_local(async move {
            let res = invoke::<LlmProfile, _>(
                "create_llm_profile",
                &CreateProfileArgs {
                    name: form_name.get(),
                    protocol: form_protocol.get(),
                    base_url: form_base_url.get(),
                    model: form_model.get(),
                    api_key: form_api_key.get(),
                    is_default: profiles.get().is_empty(),
                },
            )
            .await;

            if let Ok(new_p) = res {
                let mut list = profiles.get();
                list.push(new_p);
                profiles.set(list);

                form_name.set(String::new());
                form_api_key.set(String::new());
            }
        });
    };

    let on_delete_profile = move |id: String| {
        let p_id = id.clone();
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("delete_llm_profile", &DeleteProfileArgs { id: p_id.clone() }).await;
            let mut list = profiles.get();
            list.retain(|p| p.id != p_id);
            profiles.set(list);
        });
    };

    let on_test_profile = move |id: String| {
        test_status.set("正在测试连接...".to_string());
        leptos::task::spawn_local(async move {
            let res = invoke::<(), _>("test_llm_connection", &TestConnectionArgs { profile_id: id }).await;
            match res {
                Ok(_) => test_status.set("✓ 连接成功！模型接口响应正常".to_string()),
                Err(err) => test_status.set(format!("✕ 连接失败: {}", err)),
            }
        });
    };

    // WebDAV Actions
    let on_save_webdav = move |_| {
        webdav_status.set("正在保存并测试 WebDAV 连通性...".to_string());
        let pwd_val = webdav_pwd.get();
        let pwd_opt = if pwd_val.trim().is_empty() { None } else { Some(pwd_val) };

        leptos::task::spawn_local(async move {
            let save_res = invoke::<(), _>(
                "webdav_save_config",
                &SaveWebdavArgs {
                    url: webdav_url.get(),
                    username: webdav_user.get(),
                    password: pwd_opt,
                    remote_dir: webdav_dir.get(),
                },
            )
            .await;

            if let Err(e) = save_res {
                webdav_status.set(format!("✕ 保存配置失败: {}", e));
                return;
            }

            let test_res = invoke_no_args::<()>("webdav_test_connection").await;
            match test_res {
                Ok(_) => {
                    webdav_status.set("✓ WebDAV 连接成功！配置已安全保存至系统钥匙串。".to_string());
                    // Refresh backup list
                    if let Ok(list) = invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await {
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
                    if let Ok(list) = invoke_no_args::<Vec<BackupFileInfo>>("webdav_list_backups").await {
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
        webdav_status.set(format!("正在还原版本 {}，并自动在本地留存安全快照……", fn_clone));

        leptos::task::spawn_local(async move {
            let res = invoke::<(), _>(
                "webdav_restore_backup",
                &RestoreBackupArgs { filename: fn_clone.clone() },
            )
            .await;

            match res {
                Ok(_) => {
                    webdav_status.set(format!("✓ 版本 {} 还原成功！本地旧版已留存至 backups/ 目录防覆写。", fn_clone));
                }
                Err(e) => {
                    webdav_status.set(format!("✕ 还原失败: {}", e));
                }
            }
            is_webdav_syncing.set(false);
        });
    };

    // Save Preferences
    let on_save_preferences = move |_| {
        pref_status.set("正在保存偏好设置...".to_string());
        let current_theme = theme.get();
        let current_fs = font_size.get();
        let current_lh = line_height.get();
        let current_intv = auto_save_interval.get();

        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "theme".to_string(),
                value: current_theme.clone(),
            }).await;
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "font_size".to_string(),
                value: current_fs.clone(),
            }).await;
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "line_height".to_string(),
                value: current_lh.clone(),
            }).await;
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "auto_save_interval".to_string(),
                value: current_intv.clone(),
            }).await;

            // Apply live
            app_ctx.theme.set(current_theme.clone());
            crate::bridge::theme::apply_app_theme(&current_theme);

            let fs_num = current_fs.parse::<u32>().unwrap_or(16);
            app_ctx.font_size.set(fs_num);
            app_ctx.line_height.set(current_lh.clone());
            crate::bridge::theme::set_editor_style(fs_num, &current_lh);

            if let Ok(intv_num) = current_intv.parse::<u32>() {
                app_ctx.auto_save_interval.set(intv_num);
            }

            pref_status.set("✓ 偏好设置已更新并持久化！".to_string());
        });
    };

    view! {
        {move || {
            if is_open.get() {
                let tab = active_tab.get();
                view! {
                    <div class="fixed inset-0 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center z-50 p-6 select-none">
                        <div class="bg-white dark:bg-slate-800 rounded-xl shadow-2xl max-w-4xl w-full border border-slate-200 dark:border-slate-700 overflow-hidden flex flex-col h-[650px] animate-in fade-in zoom-in-95">
                            // Header
                            <div class="px-6 py-4 border-b border-slate-200 dark:border-slate-700 flex items-center justify-between bg-slate-50 dark:bg-slate-800/80">
                                <div class="flex items-center gap-2.5">
                                    <span class="text-xl">"⚙️"</span>
                                    <h3 class="font-bold text-base text-slate-800 dark:text-slate-100">"工作台系统设置"</h3>
                                </div>
                                <button
                                    class="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 font-mono text-xl cursor-pointer"
                                    on:click=move |_| is_open.set(false)
                                >
                                    "✕"
                                </button>
                            </div>

                            // Tab navigation
                            <div class="flex border-b border-slate-200 dark:border-slate-700 bg-slate-100 dark:bg-slate-900/60 text-xs font-semibold text-slate-600 dark:text-slate-400 px-6">
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "llm" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("llm".to_string())
                                >
                                    "大模型 API 接口"
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "agent" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("agent".to_string())
                                >
                                    "多 Agent 接口绑定"
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "webdav" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("webdav".to_string())
                                >
                                    "WebDAV 整库同步"
                                </button>
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "pref" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("pref".to_string())
                                >
                                    "写作偏好"
                                </button>
                            </div>

                            // Tab contents
                            <div class="flex-1 overflow-y-auto p-6 bg-white dark:bg-slate-800">
                                {move || match tab.as_str() {
                                    "agent" => view! {
                                        <div class="space-y-4">
                                            <p class="text-xs text-slate-500">"每个模块由独立预设 Agent 负责，各 Agent 可绑定不同的大模型 API 配置。"</p>
                                            <div class="space-y-3">
                                                <For
                                                    each=move || agents.get()
                                                    key=|a| a.id.clone()
                                                    children=move |ag| {
                                                        let prompt = RwSignal::new(ag.system_prompt.clone());
                                                        view! {
                                                            <div class="border border-slate-200 rounded-lg p-3 bg-white space-y-2">
                                                                <div class="flex items-center justify-between">
                                                                    <div class="flex items-center gap-2">
                                                                        <span class="font-bold text-xs text-slate-800">{ag.name.clone()}</span>
                                                                        <span class="text-[10px] text-slate-400 font-mono">"key: " {ag.key.clone()}</span>
                                                                    </div>
                                                                    <span class="text-[10px] bg-slate-100 text-slate-600 px-2 py-0.5 rounded">
                                                                        "已启用"
                                                                    </span>
                                                                </div>
                                                                <textarea
                                                                    class="w-full text-xs text-slate-600 bg-slate-50 border border-slate-200 rounded p-2 focus:bg-white focus:outline-blue-500 min-h-[50px]"
                                                                    prop:value=move || prompt.get()
                                                                    on:input=move |ev| prompt.set(event_target_value(&ev))
                                                                ></textarea>
                                                            </div>
                                                        }
                                                    }
                                                />
                                            </div>
                                        </div>
                                    }.into_any(),

                                    "webdav" => view! {
                                        <div class="space-y-6 text-xs text-slate-700">
                                            <div class="grid grid-cols-2 gap-6">
                                                // Form
                                                <div class="space-y-3">
                                                    <h4 class="font-bold text-xs text-slate-800">"WebDAV 服务器凭据"</h4>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"WebDAV 服务器地址"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded font-mono text-xs focus:outline-blue-500"
                                                            placeholder="https://dav.jianguoyun.com/dav/"
                                                            prop:value=move || webdav_url.get()
                                                            on:input=move |ev| webdav_url.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"用户名"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded text-xs focus:outline-blue-500"
                                                            placeholder="user@example.com"
                                                            prop:value=move || webdav_user.get()
                                                            on:input=move |ev| webdav_user.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"应用专用密码 / Token（保存至系统钥匙串）"</label>
                                                        <input
                                                            type="password"
                                                            class="w-full p-2 border border-slate-300 rounded text-xs focus:outline-blue-500 font-mono"
                                                            placeholder="••••••••••••"
                                                            prop:value=move || webdav_pwd.get()
                                                            on:input=move |ev| webdav_pwd.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"远程备份目录"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded text-xs focus:outline-blue-500 font-mono"
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
                                                    <h4 class="font-bold text-xs text-slate-800">"云端备份快照历史（带防覆写恢复）"</h4>
                                                    <div class="border border-slate-200 rounded-lg p-3 bg-slate-50 min-h-[220px] max-h-[320px] overflow-y-auto space-y-2">
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
                                                                                <div class="flex items-center justify-between p-2 bg-white rounded border border-slate-200 text-xs shadow-2xs">
                                                                                    <div>
                                                                                        <div class="font-mono font-semibold text-slate-800 text-[11px]">
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
                                                    <div class="p-2.5 bg-blue-50 border border-blue-200 rounded-md text-xs font-semibold text-blue-800">
                                                        {webdav_status.get()}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div></div> }.into_any()
                                            }}
                                        </div>
                                    }.into_any(),

                                    "pref" => view! {
                                        <div class="max-w-md space-y-5 text-xs text-slate-700 dark:text-slate-300">
                                            <div class="space-y-1.5">
                                                <label class="font-semibold block text-slate-800 dark:text-slate-200">"外观主题"</label>
                                                <div class="grid grid-cols-3 gap-2">
                                                    <button
                                                        type="button"
                                                        class=move || format!(
                                                            "px-3 py-2.5 rounded border text-center transition-colors cursor-pointer {}",
                                                            if theme.get() == "system" {
                                                                "border-blue-600 bg-blue-50 text-blue-700 font-bold dark:bg-blue-950/70 dark:border-blue-500 dark:text-blue-300"
                                                            } else {
                                                                "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/60"
                                                            }
                                                        )
                                                        on:click=move |_| {
                                                            theme.set("system".to_string());
                                                            crate::bridge::theme::apply_app_theme("system");
                                                        }
                                                    >
                                                        "🌓 跟随系统"
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class=move || format!(
                                                            "px-3 py-2.5 rounded border text-center transition-colors cursor-pointer {}",
                                                            if theme.get() == "light" {
                                                                "border-blue-600 bg-blue-50 text-blue-700 font-bold dark:bg-blue-950/70 dark:border-blue-500 dark:text-blue-300"
                                                            } else {
                                                                "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/60"
                                                            }
                                                        )
                                                        on:click=move |_| {
                                                            theme.set("light".to_string());
                                                            crate::bridge::theme::apply_app_theme("light");
                                                        }
                                                    >
                                                        "☀️ 浅色模式"
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class=move || format!(
                                                            "px-3 py-2.5 rounded border text-center transition-colors cursor-pointer {}",
                                                            if theme.get() == "dark" {
                                                                "border-blue-600 bg-blue-50 text-blue-700 font-bold dark:bg-blue-950/70 dark:border-blue-500 dark:text-blue-300"
                                                            } else {
                                                                "border-slate-300 bg-white text-slate-700 hover:bg-slate-50 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700/60"
                                                            }
                                                        )
                                                        on:click=move |_| {
                                                            theme.set("dark".to_string());
                                                            crate::bridge::theme::apply_app_theme("dark");
                                                        }
                                                    >
                                                        "🌙 深色模式"
                                                    </button>
                                                </div>
                                            </div>
                                            <div class="space-y-1">
                                                <label class="font-semibold block text-slate-800 dark:text-slate-200">"正文字号 (px)"</label>
                                                <input
                                                    type="number"
                                                    class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                                                    prop:value=move || font_size.get()
                                                    on:input=move |ev| font_size.set(event_target_value(&ev))
                                                />
                                            </div>
                                            <div class="space-y-1">
                                                <label class="font-semibold block text-slate-800 dark:text-slate-200">"正文行高"</label>
                                                <input
                                                    type="text"
                                                    class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                                                    prop:value=move || line_height.get()
                                                    on:input=move |ev| line_height.set(event_target_value(&ev))
                                                />
                                            </div>
                                            <div class="space-y-1">
                                                <label class="font-semibold block text-slate-800 dark:text-slate-200">"自动保存间隔 (秒)"</label>
                                                <input
                                                    type="number"
                                                    class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500"
                                                    prop:value=move || auto_save_interval.get()
                                                    on:input=move |ev| auto_save_interval.set(event_target_value(&ev))
                                                />
                                            </div>
                                            <button
                                                class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium shadow-xs transition-colors cursor-pointer"
                                                on:click=on_save_preferences
                                            >
                                                "保存写作偏好"
                                            </button>

                                            {move || if !pref_status.get().is_empty() {
                                                view! {
                                                    <div class="p-2.5 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-md text-xs font-semibold text-emerald-800 dark:text-emerald-300">
                                                        {pref_status.get()}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div></div> }.into_any()
                                            }}
                                        </div>
                                    }.into_any(),

                                    _ => view! {
                                        // Default: LLM Profiles
                                        <div class="space-y-6">
                                            // Profile list
                                            <div class="space-y-2">
                                                <h4 class="font-bold text-xs text-slate-700">"已配置的模型接口："</h4>
                                                <For
                                                    each=move || profiles.get()
                                                    key=|p| p.id.clone()
                                                    children=move |p| {
                                                        let p_id = p.id.clone();
                                                        let p_id_for_test = p_id.clone();
                                                        let p_id_for_del = p_id.clone();
                                                        view! {
                                                            <div class="flex items-center justify-between p-3 border border-slate-200 rounded-lg bg-slate-50 text-xs">
                                                                <div class="space-y-0.5">
                                                                    <div class="flex items-center gap-2">
                                                                        <span class="font-bold text-slate-800">{p.name.clone()}</span>
                                                                        <span class="text-[10px] bg-blue-100 text-blue-800 px-1.5 py-0.2 rounded font-mono">
                                                                            {p.protocol.clone()}
                                                                        </span>
                                                                        {if p.is_default {
                                                                            view! { <span class="text-[10px] bg-emerald-100 text-emerald-800 px-1.5 py-0.2 rounded">"默认"</span> }.into_any()
                                                                        } else {
                                                                            view! { <span></span> }.into_any()
                                                                        }}
                                                                    </div>
                                                                    <div class="text-slate-400 font-mono text-[11px]">
                                                                        "URL: " {p.base_url.clone()} " | Model: " {p.model.clone()}
                                                                    </div>
                                                                </div>
                                                                <div class="flex items-center gap-2">
                                                                    <button
                                                                        class="px-2.5 py-1 bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded text-xs transition-colors cursor-pointer"
                                                                        on:click=move |_| on_test_profile(p_id_for_test.clone())
                                                                    >
                                                                        "测试连接"
                                                                    </button>
                                                                    <button
                                                                        class="px-2.5 py-1 text-red-600 hover:bg-red-50 rounded text-xs transition-colors cursor-pointer"
                                                                        on:click=move |_| on_delete_profile(p_id_for_del.clone())
                                                                    >
                                                                        "删除"
                                                                    </button>
                                                                </div>
                                                            </div>
                                                        }
                                                    }
                                                />
                                            </div>

                                            {move || if !test_status.get().is_empty() {
                                                view! {
                                                    <div class="p-2.5 bg-blue-50 border border-blue-200 rounded-md text-xs font-semibold text-blue-800">
                                                        {test_status.get()}
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div></div> }.into_any()
                                            }}

                                            // Add new profile form
                                            <div class="border-t border-slate-200 pt-4 space-y-3">
                                                <h4 class="font-bold text-xs text-slate-800">"+ 添加新模型接口配置"</h4>
                                                <div class="grid grid-cols-2 gap-3 text-xs">
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"配置名称"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500"
                                                            placeholder="如：DeepSeek / Claude 3.5 / 本地 Ollama"
                                                            prop:value=move || form_name.get()
                                                            on:input=move |ev| form_name.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"协议规范"</label>
                                                        <select
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 bg-white"
                                                            on:change=move |ev| form_protocol.set(event_target_value(&ev))
                                                        >
                                                            <option value="openai">"OpenAI 兼容协议"</option>
                                                            <option value="anthropic">"Anthropic 兼容协议"</option>
                                                        </select>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"Base URL 接口地址"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="https://api.openai.com/v1"
                                                            prop:value=move || form_base_url.get()
                                                            on:input=move |ev| form_base_url.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"模型名称 (Model)"</label>
                                                        <input
                                                            type="text"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="gpt-4o / deepseek-chat / claude-3-5-sonnet"
                                                            prop:value=move || form_model.get()
                                                            on:input=move |ev| form_model.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="col-span-2 space-y-1">
                                                        <label class="font-medium text-slate-600">"API Key 密钥（保存至系统钥匙串，界面掩码，不随整库上传）"</label>
                                                        <input
                                                            type="password"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="sk-••••••••••••••••••••••••••••••••"
                                                            prop:value=move || form_api_key.get()
                                                            on:input=move |ev| form_api_key.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                </div>
                                                <button
                                                    class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium text-xs shadow-xs transition-colors cursor-pointer"
                                                    on:click=on_add_profile
                                                >
                                                    "保存此接口配置"
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any(),
                                }}
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <div></div> }.into_any()
            }
        }}
    }
}

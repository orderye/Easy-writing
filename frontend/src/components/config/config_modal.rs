use crate::state::AppContext;
use crate::tauri_api::{invoke, invoke_no_args};
use leptos::prelude::*;
use serde::Serialize;
use shared_types::{
    Agent, AppUpdateInfo, BackupFileInfo, GenerationParams, LlmProfile, PromptTemplate, RemoteModel,
    WebdavConfig,
};

#[derive(Serialize)]
struct CreateProfileArgs {
    name: String,
    protocol: String,
    base_url: String,
    model: String,
    api_key: String,
    is_default: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: Option<bool>,
}

#[derive(Serialize)]
struct UpdateProfileArgs {
    id: String,
    name: String,
    protocol: String,
    base_url: String,
    model: String,
    is_default: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: Option<bool>,
}

#[derive(Serialize)]
struct FetchModelsArgs {
    protocol: String,
    base_url: String,
    api_key: Option<String>,
    profile_id: Option<String>,
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

#[derive(Serialize)]
struct CheckUpdateArgs {
    force: bool,
}

#[derive(Serialize)]
struct OpenUrlArgs {
    url: String,
}

/// 把 llm_profile.params 里的生成参数渲染成一行摘要文案（PRD §5.1 P0）。
fn gen_params_summary(params_json: &str) -> String {
    let params = GenerationParams::parse_from_str(params_json);
    if params.is_empty() {
        return "生成参数：接口默认（temperature / 最大输出长度 / 流式均未覆盖）".to_string();
    }

    let temp = params
        .temperature
        .map(|t| t.to_string())
        .unwrap_or_else(|| "默认".to_string());
    let max_tokens = params
        .max_tokens
        .map(|m| m.to_string())
        .unwrap_or_else(|| "默认".to_string());
    let stream = params.stream.unwrap_or(true);

    format!(
        "生成参数：temperature {} · 最大输出长度 {} · {}",
        temp,
        max_tokens,
        if stream { "流式输出" } else { "一次性返回" }
    )
}

#[component]
pub fn ConfigModal(is_open: RwSignal<bool>, app_ctx: AppContext) -> impl IntoView {
    let active_tab = RwSignal::new("llm".to_string()); // "llm" | "agent" | "prompt" | "webdav" | "pref"
    let profiles = RwSignal::new(Vec::<LlmProfile>::new());
    let agents = RwSignal::new(Vec::<Agent>::new());
    let prompt_templates = RwSignal::new(Vec::<PromptTemplate>::new());

    // New profile form state
    let form_name = RwSignal::new(String::new());
    let form_protocol = RwSignal::new("openai".to_string());
    let form_base_url = RwSignal::new("https://api.openai.com/v1".to_string());
    let form_model = RwSignal::new("gpt-4o".to_string());
    let form_api_key = RwSignal::new(String::new());

    // 生成参数（PRD §5.1 P0）：temperature、最大输出长度、是否流式
    let form_temperature = RwSignal::new(String::new());
    let form_max_tokens = RwSignal::new(String::new());
    let form_stream = RwSignal::new(true);

    // 「拉取模型列表」（PRD §5.1 P0）
    let remote_models = RwSignal::new(Vec::<RemoteModel>::new());
    let is_fetching_models = RwSignal::new(false);

    // 编辑已有配置时的生成参数（进入编辑态后展示）
    let editing_id = RwSignal::new(Option::<String>::None);
    let edit_temperature = RwSignal::new(String::new());
    let edit_max_tokens = RwSignal::new(String::new());
    let edit_stream = RwSignal::new(true);

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
    // 保存章节后自动提炼（PRD §4.2 P1，默认关闭）
    let auto_extract = RwSignal::new(false);
    let pref_status = RwSignal::new(String::new());

    // App Update state
    let auto_check_update = RwSignal::new(true);
    let custom_update_url = RwSignal::new(String::new());
    let update_status_msg = RwSignal::new(String::new());
    let last_check_time_str = RwSignal::new(String::new());

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

            // Load Prompt Templates
            if let Ok(list) = invoke_no_args::<Vec<PromptTemplate>>("list_prompt_templates").await {
                prompt_templates.set(list);
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
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "auto_extract".to_string() }).await {
                auto_extract.set(val == "true" || val == "1");
            }

            // Load Update Preferences
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "auto_check_update".to_string() }).await {
                auto_check_update.set(val != "false");
            }
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "update_check_url".to_string() }).await {
                custom_update_url.set(val);
            }
            if let Ok(Some(val)) = invoke::<Option<String>, _>("get_app_setting", &GetAppSettingArgs { key: "last_update_check_time".to_string() }).await {
                last_check_time_str.set(val);
            }
        });
    };

    Effect::new(move |_| {
        if is_open.get() {
            load_data();
        }
    });

    // 把表单里的可选数字解析为 Option，空字符串表示「未设置 / 使用默认」
    let parse_opt_f32 = |raw: String| -> Result<Option<f32>, String> {
        let t = raw.trim();
        if t.is_empty() {
            Ok(None)
        } else {
            t.parse::<f32>()
                .map(Some)
                .map_err(|_| "temperature 需为 0 ~ 2 之间的数字".to_string())
        }
    };
    let parse_opt_u32 = |raw: String| -> Result<Option<u32>, String> {
        let t = raw.trim();
        if t.is_empty() {
            Ok(None)
        } else {
            t.parse::<u32>()
                .map(Some)
                .map_err(|_| "最大输出长度需为正整数".to_string())
        }
    };

    let on_add_profile = move |_| {
        let key_val = form_api_key.get().trim().to_string();
        if key_val.is_empty() {
            test_status.set("✕ 拦截提示：未填写 API Key 密钥，请输入以 sk- 开头的有效密钥后再保存配置".to_string());
            return;
        }

        let temperature = match parse_opt_f32(form_temperature.get()) {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };
        let max_tokens = match parse_opt_u32(form_max_tokens.get()) {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };

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
                    temperature,
                    max_tokens,
                    stream: Some(form_stream.get()),
                },
            )
            .await;

            match res {
                Ok(new_p) => {
                    let mut list = profiles.get();
                    list.push(new_p);
                    profiles.set(list);

                    form_name.set(String::new());
                    form_api_key.set(String::new());
                    form_temperature.set(String::new());
                    form_max_tokens.set(String::new());
                    form_stream.set(true);
                    remote_models.set(Vec::new());
                    test_status.set("✓ 接口配置已保存至钥匙串，可点击上方「测试连接」验证连通性".to_string());
                }
                Err(err) => {
                    test_status.set(format!("✕ 保存失败: {}", err));
                }
            }
        });
    };

    // 「拉取模型列表」：用当前表单的协议 / Base URL / API Key 拉取远端可用模型
    let on_fetch_models = move |_| {
        is_fetching_models.set(true);
        test_status.set("正在拉取模型列表...".to_string());

        let protocol = form_protocol.get();
        let base_url = form_base_url.get();
        let key = form_api_key.get().trim().to_string();
        let api_key = if key.is_empty() { None } else { Some(key) };

        leptos::task::spawn_local(async move {
            let res = invoke::<Vec<RemoteModel>, _>(
                "fetch_remote_models",
                &FetchModelsArgs {
                    protocol,
                    base_url,
                    api_key,
                    profile_id: None,
                },
            )
            .await;

            match res {
                Ok(list) => {
                    let n = list.len();
                    remote_models.set(list);
                    test_status.set(format!("✓ 已拉取到 {} 个可用模型，点击下方列表即可选用", n));
                }
                Err(err) => {
                    remote_models.set(Vec::new());
                    test_status.set(format!("✕ 拉取模型列表失败: {}", err));
                }
            }
            is_fetching_models.set(false);
        });
    };

    // 保存已有配置的生成参数（PRD §5.1 P0）
    let on_save_profile_params = move |profile: LlmProfile| {
        let temperature = match parse_opt_f32(edit_temperature.get()) {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };
        let max_tokens = match parse_opt_u32(edit_max_tokens.get()) {
            Ok(v) => v,
            Err(msg) => {
                test_status.set(format!("✕ {}", msg));
                return;
            }
        };

        let stream = Some(edit_stream.get());
        leptos::task::spawn_local(async move {
            let res = invoke::<LlmProfile, _>(
                "update_llm_profile",
                &UpdateProfileArgs {
                    id: profile.id.clone(),
                    name: profile.name.clone(),
                    protocol: profile.protocol.clone(),
                    base_url: profile.base_url.clone(),
                    model: profile.model.clone(),
                    is_default: profile.is_default,
                    temperature,
                    max_tokens,
                    stream,
                },
            )
            .await;

            match res {
                Ok(updated) => {
                    let mut list = profiles.get();
                    if let Some(slot) = list.iter_mut().find(|p| p.id == updated.id) {
                        *slot = updated.clone();
                    }
                    profiles.set(list);
                    editing_id.set(None);
                    test_status.set("✓ 生成参数已保存，后续调用该接口时立即生效".to_string());
                }
                Err(err) => {
                    test_status.set(format!("✕ 保存生成参数失败: {}", err));
                }
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
        let current_auto_extract = auto_extract.get();

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
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "auto_extract".to_string(),
                value: if current_auto_extract { "true".to_string() } else { "false".to_string() },
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

    let on_check_update_now = move |_| {
        app_ctx.is_checking_update.set(true);
        update_status_msg.set("正在连接远程版本源检查更新...".to_string());
        leptos::task::spawn_local(async move {
            let res = invoke::<AppUpdateInfo, _>("check_app_update", &CheckUpdateArgs { force: true }).await;
            app_ctx.is_checking_update.set(false);
            match res {
                Ok(info) => {
                    if info.has_update {
                        update_status_msg.set(format!("🎉 发现新版本 v{}！已为您打开更新详情", info.latest_version));
                        app_ctx.available_update.set(Some(info));
                        app_ctx.has_update_badge.set(true);
                        app_ctx.is_update_modal_open.set(true);
                    } else {
                        update_status_msg.set(format!("✓ 当前已是最新版本 (v{})，暂无新版本发布", info.current_version));
                    }
                }
                Err(e) => {
                    update_status_msg.set(format!("✕ 检查更新失败：{}", e));
                }
            }
        });
    };

    let on_save_update_settings = move |_| {
        let is_auto = auto_check_update.get();
        let custom_url = custom_update_url.get().trim().to_string();
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "auto_check_update".to_string(),
                value: if is_auto { "true".to_string() } else { "false".to_string() },
            }).await;
            let _ = invoke::<(), _>("set_app_setting", &SetAppSettingArgs {
                key: "update_check_url".to_string(),
                value: custom_url,
            }).await;
            update_status_msg.set("✓ 更新检测设置已保存！".to_string());
        });
    };

    let on_open_external = move |url: String| {
        leptos::task::spawn_local(async move {
            let _ = invoke::<(), _>("open_external_url", &OpenUrlArgs { url }).await;
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
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer {}", if tab == "prompt" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("prompt".to_string())
                                >
                                    "修饰提示词模板"
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
                                <button
                                    class=format!("py-3 px-4 border-b-2 transition-colors cursor-pointer flex items-center gap-1.5 {}", if tab == "update" { "border-blue-600 text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-800" } else { "border-transparent hover:text-slate-900 dark:hover:text-slate-200" })
                                    on:click=move |_| active_tab.set("update".to_string())
                                >
                                    <span>"关于与更新"</span>
                                    {move || if app_ctx.has_update_badge.get() {
                                        view! { <span class="px-1.5 py-0.2 rounded-full text-[9px] bg-red-500 text-white font-bold leading-none animate-pulse">"新版"</span> }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }}
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

                                    "prompt" => view! {
                                        <div class="space-y-4">
                                            <div class="flex items-center justify-between pb-2 border-b border-slate-200 dark:border-slate-700">
                                                <div>
                                                    <h4 class="text-xs font-bold text-slate-800 dark:text-slate-100">"6 处修饰模块独立 Prompt 模板"</h4>
                                                    <p class="text-[11px] text-slate-500 dark:text-slate-400">"自定义世界观、角色卡、剧情伏笔、全篇梗概、卷简介及章摘要的 AI 润色修饰指令，支持一键恢复出厂预设。"</p>
                                                </div>
                                            </div>

                                            <div class="space-y-3">
                                                {move || {
                                                    let tmpls = prompt_templates.get();
                                                    tmpls.into_iter().map(|item| {
                                                        let key = StoredValue::new(item.key);
                                                        let name = item.name;
                                                        let def_tmpl = StoredValue::new(item.default_template);
                                                        let tmpl_signal = RwSignal::new(item.template);
                                                        let save_msg = RwSignal::new(String::new());

                                                        view! {
                                                            <div class="border border-slate-200 dark:border-slate-700 rounded-lg p-3 bg-slate-50/50 dark:bg-slate-850/50 space-y-2">
                                                                <div class="flex items-center justify-between">
                                                                    <div class="flex items-center gap-2">
                                                                        <span class="font-bold text-xs text-slate-800 dark:text-slate-100">{name}</span>
                                                                        <span class="text-[10px] font-mono px-1.5 py-0.2 bg-slate-200 dark:bg-slate-750 text-slate-600 dark:text-slate-300 rounded">
                                                                            {move || key.get_value()}
                                                                        </span>
                                                                        {move || if tmpl_signal.get() != def_tmpl.get_value() {
                                                                            view! {
                                                                                <span class="text-[10px] px-1.5 py-0.2 bg-amber-100 dark:bg-amber-950/60 text-amber-800 dark:text-amber-300 rounded border border-amber-200 dark:border-amber-700">
                                                                                    "已自定义"
                                                                                </span>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! {
                                                                                <span class="text-[10px] px-1.5 py-0.2 bg-slate-100 dark:bg-slate-800 text-slate-400 rounded">
                                                                                    "默认预设"
                                                                                </span>
                                                                            }.into_any()
                                                                        }}
                                                                    </div>
                                                                    <div class="flex items-center gap-2">
                                                                        <span class="text-[11px] text-emerald-600 dark:text-emerald-400">{move || save_msg.get()}</span>
                                                                        {move || if tmpl_signal.get() != def_tmpl.get_value() {
                                                                            view! {
                                                                                <button
                                                                                    class="text-xs text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200 hover:bg-slate-200 dark:hover:bg-slate-700 px-2 py-1 rounded transition-colors cursor-pointer"
                                                                                    on:click=move |_| {
                                                                                        let k = key.get_value();
                                                                                        let dt = def_tmpl.get_value();
                                                                                        leptos::task::spawn_local(async move {
                                                                                            #[derive(Serialize)]
                                                                                            struct ResetTmplArgs {
                                                                                                key: String,
                                                                                            }
                                                                                            if invoke::<(), _>("reset_prompt_template", &ResetTmplArgs { key: k }).await.is_ok() {
                                                                                                tmpl_signal.set(dt);
                                                                                                save_msg.set("已恢复出厂默认预设 ✓".to_string());
                                                                                            }
                                                                                        });
                                                                                    }
                                                                                >
                                                                                    "恢复默认"
                                                                                </button>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! { <div></div> }.into_any()
                                                                        }}
                                                                        <button
                                                                            class="text-xs bg-blue-600 hover:bg-blue-700 text-white px-3 py-1 rounded font-medium shadow-xs transition-colors cursor-pointer"
                                                                            on:click=move |_| {
                                                                                let k = key.get_value();
                                                                                let t = tmpl_signal.get();
                                                                                leptos::task::spawn_local(async move {
                                                                                    #[derive(Serialize)]
                                                                                    struct SaveTmplArgs {
                                                                                        key: String,
                                                                                        template: String,
                                                                                    }
                                                                                    if invoke::<(), _>("save_prompt_template", &SaveTmplArgs { key: k, template: t }).await.is_ok() {
                                                                                        save_msg.set("已保存自定义模板 ✓".to_string());
                                                                                    }
                                                                                });
                                                                            }
                                                                        >
                                                                            "保存修改"
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                                <textarea
                                                                    class="w-full text-xs text-slate-700 dark:text-slate-200 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md p-2 min-h-[70px] leading-relaxed font-mono focus:outline-hidden focus:ring-1 focus:ring-blue-500"
                                                                    prop:value=move || tmpl_signal.get()
                                                                    on:input=move |ev| tmpl_signal.set(event_target_value(&ev))
                                                                ></textarea>
                                                            </div>
                                                        }
                                                    }).collect::<Vec<_>>()
                                                }}
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
                                            <div class="flex items-start justify-between gap-4 p-3 rounded-lg border border-slate-200 dark:border-slate-700">
                                                <div>
                                                    <label class="font-semibold block text-slate-800 dark:text-slate-200">"保存章节后自动提炼"</label>
                                                    <span class="text-[11px] text-slate-400 dark:text-slate-500">
                                                        "开启后，保存正文时后台静默提炼世界观 / 角色 / 剧情 / 章摘要候选；默认关闭，仅手动点「提炼本章」。"</span>
                                                </div>
                                                <input
                                                    type="checkbox"
                                                    class="w-4 h-4 mt-0.5 text-blue-600 rounded cursor-pointer shrink-0"
                                                    prop:checked=move || auto_extract.get()
                                                    on:change=move |ev| auto_extract.set(event_target_checked(&ev))
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

                                    "update" => view! {
                                        <div class="space-y-6 max-w-2xl">
                                            // Software Branding & Version Info Card
                                            <div class="flex items-center gap-4 p-4 rounded-xl bg-slate-50 dark:bg-slate-900/60 border border-slate-200 dark:border-slate-700/80">
                                                <div class="w-14 h-14 rounded-2xl bg-gradient-to-tr from-blue-600 to-indigo-500 flex items-center justify-center text-2xl text-white shadow-md shrink-0">
                                                    "✍️"
                                                </div>
                                                <div class="space-y-1">
                                                    <div class="flex items-center gap-2">
                                                        <h4 class="font-bold text-sm text-slate-800 dark:text-slate-100">"Easy-writing · 小说工作台"</h4>
                                                        <span class="px-2 py-0.5 rounded-full text-[10px] font-mono bg-blue-100 dark:bg-blue-900/60 text-blue-700 dark:text-blue-300 font-semibold border border-blue-200 dark:border-blue-800">
                                                            "v0.2.0"
                                                        </span>
                                                    </div>
                                                    <p class="text-xs text-slate-500 dark:text-slate-400">
                                                        "面向中长篇连载小说作者的专业级本地离线写作工作台 · AI 辅助提炼与设定协同"
                                                    </p>
                                                </div>
                                            </div>

                                            // Update Actions & Status Card
                                            <div class="p-4 rounded-xl border border-slate-200 dark:border-slate-700 space-y-4 bg-white dark:bg-slate-800">
                                                <div class="flex items-center justify-between">
                                                    <div>
                                                        <h5 class="font-bold text-xs text-slate-800 dark:text-slate-200">"版本在线更新检查"</h5>
                                                        <p class="text-[11px] text-slate-400 dark:text-slate-500">
                                                            {move || {
                                                                let time = last_check_time_str.get();
                                                                if time.is_empty() {
                                                                    "尚未检查过更新".to_string()
                                                                } else {
                                                                    format!("上次检测时间：{}", time.chars().take(19).collect::<String>().replace('T', " "))
                                                                }
                                                            }}
                                                        </p>
                                                    </div>
                                                    <button
                                                        class=move || {
                                                            format!(
                                                                "px-3.5 py-1.5 rounded-lg text-xs font-semibold shadow-xs transition-colors flex items-center gap-1.5 cursor-pointer {}",
                                                                if app_ctx.is_checking_update.get() {
                                                                    "bg-slate-200 dark:bg-slate-700 text-slate-400 cursor-not-allowed"
                                                                } else {
                                                                    "bg-blue-600 hover:bg-blue-700 active:bg-blue-800 text-white"
                                                                }
                                                            )
                                                        }
                                                        disabled=move || app_ctx.is_checking_update.get()
                                                        on:click=on_check_update_now
                                                    >
                                                        {move || if app_ctx.is_checking_update.get() {
                                                            view! {
                                                                <span>"🔄 检测中..."</span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span>"🔍 立即检查新版本"</span>
                                                            }.into_any()
                                                        }}
                                                    </button>
                                                </div>

                                                // Update status message banner
                                                {move || {
                                                    let msg = update_status_msg.get();
                                                    if msg.is_empty() {
                                                        view! { <div></div> }.into_any()
                                                    } else if msg.starts_with('✓') {
                                                        view! {
                                                            <div class="p-2.5 rounded-lg bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 text-xs text-emerald-800 dark:text-emerald-300 font-medium">
                                                                {msg}
                                                            </div>
                                                        }.into_any()
                                                    } else if msg.starts_with('✕') {
                                                        view! {
                                                            <div class="p-2.5 rounded-lg bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-800 text-xs text-red-800 dark:text-red-300 font-medium">
                                                                {msg}
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="p-2.5 rounded-lg bg-blue-50 dark:bg-blue-950/40 border border-blue-200 dark:border-blue-800 text-xs text-blue-800 dark:text-blue-300 font-medium">
                                                                {msg}
                                                            </div>
                                                        }.into_any()
                                                    }
                                                }}

                                                // If an update is detected, show shortcut banner
                                                {move || {
                                                    if let Some(info) = app_ctx.available_update.get() {
                                                        let info_clone = info.clone();
                                                        view! {
                                                            <div class="p-3 bg-gradient-to-r from-blue-50 to-indigo-50 dark:from-blue-950/40 dark:to-indigo-950/40 border border-blue-200 dark:border-blue-800 rounded-xl flex items-center justify-between">
                                                                <div>
                                                                    <div class="font-bold text-xs text-blue-900 dark:text-blue-200">
                                                                        {format!("🎉 现已发布新版本：v{} ({})", info_clone.latest_version, info_clone.release_name)}
                                                                    </div>
                                                                    <div class="text-[11px] text-blue-600 dark:text-blue-400">
                                                                        "点击右侧按钮查看详细更新日志并前往下载安装。"
                                                                    </div>
                                                                </div>
                                                                <button
                                                                    class="px-3 py-1 rounded bg-blue-600 hover:bg-blue-700 text-white font-medium text-xs transition-colors shrink-0 cursor-pointer"
                                                                    on:click=move |_| app_ctx.is_update_modal_open.set(true)
                                                                >
                                                                    "查看更新详情"
                                                                </button>
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! { <div></div> }.into_any()
                                                    }
                                                }}
                                            </div>

                                            // Update Preferences Form
                                            <div class="p-4 rounded-xl border border-slate-200 dark:border-slate-700 space-y-3.5 bg-slate-50/50 dark:bg-slate-900/40">
                                                <h5 class="font-bold text-xs text-slate-800 dark:text-slate-200">"更新选项与远程源配置"</h5>

                                                <div class="flex items-center justify-between">
                                                    <div>
                                                        <label class="font-medium text-xs text-slate-700 dark:text-slate-300 block">"应用启动时自动检测新版本"</label>
                                                        <span class="text-[11px] text-slate-400">"工作台启动后将在后台静默检测，发现更新时以徽标与弹窗提醒"</span>
                                                    </div>
                                                    <input
                                                        type="checkbox"
                                                        class="w-4 h-4 text-blue-600 rounded cursor-pointer"
                                                        prop:checked=move || auto_check_update.get()
                                                        on:change=move |ev| auto_check_update.set(event_target_checked(&ev))
                                                    />
                                                </div>

                                                <div class="space-y-1">
                                                    <label class="font-medium text-xs text-slate-700 dark:text-slate-300 block">"自定义检测更新 API 源 (可选)"</label>
                                                    <input
                                                        type="text"
                                                        class="w-full p-2 border border-slate-300 dark:border-slate-600 rounded bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-100 focus:outline-blue-500 font-mono text-xs"
                                                        placeholder="默认使用官方 GitHub Releases API (留空即为默认)"
                                                        prop:value=move || custom_update_url.get()
                                                        on:input=move |ev| custom_update_url.set(event_target_value(&ev))
                                                    />
                                                    <p class="text-[10px] text-slate-400">"若在国内网络无法直接访问 GitHub，可配置镜像反代地址。"</p>
                                                </div>

                                                <button
                                                    class="px-3.5 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium text-xs transition-colors cursor-pointer"
                                                    on:click=on_save_update_settings
                                                >
                                                    "保存更新检测偏好"
                                                </button>
                                            </div>

                                            // Links & Project Info
                                            <div class="pt-2 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 border-t border-slate-200 dark:border-slate-700">
                                                <div class="flex items-center gap-3">
                                                    <button
                                                        class="hover:text-blue-600 dark:hover:text-blue-400 underline cursor-pointer"
                                                        on:click=move |_| on_open_external("https://github.com/orderye/Easy-writing".to_string())
                                                    >
                                                        "GitHub 开源仓库"
                                                    </button>
                                                    <span>"·"</span>
                                                    <button
                                                        class="hover:text-blue-600 dark:hover:text-blue-400 underline cursor-pointer"
                                                        on:click=move |_| on_open_external("https://github.com/orderye/Easy-writing/releases".to_string())
                                                    >
                                                        "历史版本 Releases"
                                                    </button>
                                                    <span>"·"</span>
                                                    <button
                                                        class="hover:text-blue-600 dark:hover:text-blue-400 underline cursor-pointer"
                                                        on:click=move |_| on_open_external("https://github.com/orderye/Easy-writing/issues".to_string())
                                                    >
                                                        "问题反馈与建议"
                                                    </button>
                                                </div>
                                                <span class="font-mono text-[10px] text-slate-400">"MIT License"</span>
                                            </div>
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
                                                        let p_for_edit = p.clone();
                                                        let p_for_save = p.clone();

                                                        // 该配置已保存的生成参数（PRD §5.1 P0）
                                                        let saved_params = GenerationParams::parse_from_str(&p.params);
                                                        let saved_temp_str = saved_params
                                                            .temperature
                                                            .map(|t| t.to_string())
                                                            .unwrap_or_default();
                                                        let saved_max_str = saved_params
                                                            .max_tokens
                                                            .map(|t| t.to_string())
                                                            .unwrap_or_default();
                                                        let saved_stream = saved_params.stream.unwrap_or(true);
                                                        let is_editing = {
                                                            let id = p_id.clone();
                                                            move || editing_id.get().as_deref() == Some(id.as_str())
                                                        };
                                                        let is_editing_label = is_editing.clone();
                                                        let is_editing_panel = is_editing.clone();

                                                        view! {
                                                            <div class="p-3 border border-slate-200 rounded-lg bg-slate-50 text-xs space-y-2">
                                                                <div class="flex items-center justify-between">
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
                                                                        <div class="text-slate-400 text-[10px]">
                                                                            {gen_params_summary(&p.params)}
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
                                                                            class="px-2.5 py-1 bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded text-xs transition-colors cursor-pointer"
                                                                            on:click={
                                                                                let id = p_id.clone();
                                                                                let t = saved_temp_str.clone();
                                                                                let m = saved_max_str.clone();
                                                                                move |_| {
                                                                                    if editing_id.get().as_deref() == Some(id.as_str()) {
                                                                                        editing_id.set(None);
                                                                                    } else {
                                                                                        edit_temperature.set(t.clone());
                                                                                        edit_max_tokens.set(m.clone());
                                                                                        edit_stream.set(saved_stream);
                                                                                        editing_id.set(Some(id.clone()));
                                                                                    }
                                                                                }
                                                                            }
                                                                        >
                                                                            {move || if is_editing_label() { "收起" } else { "生成参数" }}
                                                                        </button>
                                                                        <button
                                                                            class="px-2.5 py-1 text-red-600 hover:bg-red-50 rounded text-xs transition-colors cursor-pointer"
                                                                            on:click=move |_| on_delete_profile(p_id_for_del.clone())
                                                                        >
                                                                            "删除"
                                                                        </button>
                                                                    </div>
                                                                </div>

                                                                // 生成参数编辑区（PRD §5.1 P0）
                                                                {move || if is_editing_panel() {
                                                                    let p_save = p_for_save.clone();
                                                                    let _ = &p_for_edit;
                                                                    view! {
                                                                        <div class="grid grid-cols-3 gap-3 pt-2 border-t border-slate-200 items-end">
                                                                            <div class="space-y-1">
                                                                                <label class="font-medium text-slate-600">"temperature"</label>
                                                                                <input
                                                                                    type="number"
                                                                                    step="0.1"
                                                                                    min="0"
                                                                                    max="2"
                                                                                    class="w-full p-1.5 border border-slate-300 rounded font-mono"
                                                                                    placeholder="留空用默认"
                                                                                    prop:value=move || edit_temperature.get()
                                                                                    on:input=move |ev| edit_temperature.set(event_target_value(&ev))
                                                                                />
                                                                            </div>
                                                                            <div class="space-y-1">
                                                                                <label class="font-medium text-slate-600">"最大输出长度"</label>
                                                                                <input
                                                                                    type="number"
                                                                                    min="1"
                                                                                    class="w-full p-1.5 border border-slate-300 rounded font-mono"
                                                                                    placeholder="留空用默认"
                                                                                    prop:value=move || edit_max_tokens.get()
                                                                                    on:input=move |ev| edit_max_tokens.set(event_target_value(&ev))
                                                                                />
                                                                            </div>
                                                                            <div class="flex items-center gap-2 pb-1.5">
                                                                                <input
                                                                                    type="checkbox"
                                                                                    class="w-3.5 h-3.5 text-blue-600 rounded cursor-pointer"
                                                                                    prop:checked=move || edit_stream.get()
                                                                                    on:change=move |ev| edit_stream.set(event_target_checked(&ev))
                                                                                />
                                                                                <label class="font-medium text-slate-600">"启用流式输出"</label>
                                                                                <button
                                                                                    class="ml-auto px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white rounded font-medium transition-colors cursor-pointer"
                                                                                    on:click=move |_| on_save_profile_params(p_save.clone())
                                                                                >
                                                                                    "保存参数"
                                                                                </button>
                                                                            </div>
                                                                        </div>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <div></div> }.into_any()
                                                                }}
                                                            </div>
                                                        }
                                                    }
                                                />
                                            </div>

                                            {move || {
                                                let msg = test_status.get();
                                                if msg.is_empty() {
                                                    view! { <div></div> }.into_any()
                                                } else if msg.starts_with('✓') {
                                                    view! {
                                                        <div class="p-2.5 bg-emerald-50 border border-emerald-200 rounded-md text-xs font-semibold text-emerald-800">
                                                            {msg}
                                                        </div>
                                                    }.into_any()
                                                } else if msg.starts_with('✕') {
                                                    view! {
                                                        <div class="p-2.5 bg-red-50 border border-red-200 rounded-md text-xs font-semibold text-red-800">
                                                            {msg}
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <div class="p-2.5 bg-blue-50 border border-blue-200 rounded-md text-xs font-semibold text-blue-800">
                                                            {msg}
                                                        </div>
                                                    }.into_any()
                                                }
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
                                                        <div class="flex items-center gap-2">
                                                            <input
                                                                type="text"
                                                                class="flex-1 p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                                placeholder="gpt-4o / deepseek-chat / claude-3-5-sonnet"
                                                                prop:value=move || form_model.get()
                                                                on:input=move |ev| form_model.set(event_target_value(&ev))
                                                            />
                                                            <button
                                                                class="px-3 py-2 bg-white border border-slate-300 hover:bg-slate-100 text-slate-700 rounded font-medium whitespace-nowrap transition-colors cursor-pointer disabled:opacity-50"
                                                                title="按上方协议与 Base URL 拉取该接口的可用模型"
                                                                disabled=move || is_fetching_models.get()
                                                                on:click=on_fetch_models
                                                            >
                                                                {move || if is_fetching_models.get() { "拉取中..." } else { "拉取模型列表" }}
                                                            </button>
                                                        </div>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"temperature"</label>
                                                        <input
                                                            type="number"
                                                            step="0.1"
                                                            min="0"
                                                            max="2"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="留空使用接口默认值"
                                                            prop:value=move || form_temperature.get()
                                                            on:input=move |ev| form_temperature.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="font-medium text-slate-600">"最大输出长度 (max_tokens)"</label>
                                                        <input
                                                            type="number"
                                                            min="1"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="留空使用接口默认值"
                                                            prop:value=move || form_max_tokens.get()
                                                            on:input=move |ev| form_max_tokens.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="col-span-2 space-y-1">
                                                        <label class="font-medium text-slate-600">"API Key 密钥（保存至系统钥匙串，界面掩码，不随整库上传）"</label>
                                                        <input
                                                            type="password"
                                                            class="w-full p-2 border border-slate-300 rounded focus:outline-blue-500 font-mono"
                                                            placeholder="请输入以 sk- 开头的 API Key"
                                                            prop:value=move || form_api_key.get()
                                                            on:input=move |ev| form_api_key.set(event_target_value(&ev))
                                                        />
                                                    </div>
                                                    <div class="col-span-2 flex items-center gap-2">
                                                        <input
                                                            type="checkbox"
                                                            class="w-3.5 h-3.5 text-blue-600 rounded cursor-pointer"
                                                            prop:checked=move || form_stream.get()
                                                            on:change=move |ev| form_stream.set(event_target_checked(&ev))
                                                        />
                                                        <label class="font-medium text-slate-600">"启用流式输出（关闭后一次性返回完整结果）"</label>
                                                    </div>
                                                </div>

                                                // 拉取到的模型候选（PRD §5.1 P0）
                                                {move || {
                                                    let models = remote_models.get();
                                                    if models.is_empty() {
                                                        view! { <div></div> }.into_any()
                                                    } else {
                                                        view! {
                                                            <div class="border border-slate-200 rounded-lg bg-white max-h-44 overflow-y-auto">
                                                                <div class="px-3 py-1.5 bg-slate-100 text-[10px] text-slate-500 font-semibold sticky top-0">
                                                                    {format!("可选模型（{} 个，点击即填入模型名称）", models.len())}
                                                                </div>
                                                                <For
                                                                    each=move || remote_models.get()
                                                                    key=|m| m.id.clone()
                                                                    children=move |m| {
                                                                        let model_id = m.id.clone();
                                                                        let owned = m.owned_by.clone();
                                                                        view! {
                                                                            <button
                                                                                class="w-full text-left px-3 py-1.5 hover:bg-blue-50 border-b border-slate-100 last:border-b-0 flex items-center justify-between gap-2 cursor-pointer"
                                                                                on:click=move |_| form_model.set(model_id.clone())
                                                                            >
                                                                                <span class="font-mono text-slate-700 truncate">{m.id.clone()}</span>
                                                                                <span class="text-[10px] text-slate-400 shrink-0">{owned.clone()}</span>
                                                                            </button>
                                                                        }
                                                                    }
                                                                />
                                                            </div>
                                                        }.into_any()
                                                    }
                                                }}

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

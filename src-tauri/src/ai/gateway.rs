use crate::ai::anthropic::{self, AnthropicMessage};
use crate::ai::openai::{self, ChatMessage};
use reqwest::Client;
use shared_types::{GenerationParams, LlmProfile};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;

/// 解析接口配置中的生成参数（temperature / 最大输出长度 / 是否流式）。
/// 参数缺省或解析失败时返回空参数，由各协议客户端使用默认值。
pub fn generation_params_of(profile: &LlmProfile) -> GenerationParams {
    GenerationParams::parse_from_str(&profile.params)
}

#[derive(Clone)]
pub struct AiGateway {
    client: Client,
}

impl AiGateway {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn test_connection(
        &self,
        profile: &LlmProfile,
        api_key: &str,
    ) -> Result<(), String> {
        match profile.protocol.as_str() {
            "anthropic" => {
                anthropic::test_anthropic_connection(
                    &self.client,
                    &profile.base_url,
                    api_key,
                    &profile.model,
                )
                .await
            }
            _ => {
                openai::test_openai_connection(
                    &self.client,
                    &profile.base_url,
                    api_key,
                    &profile.model,
                )
                .await
            }
        }
    }

    /// 「拉取模型列表」（PRD §5.1 P0）：
    /// OpenAI 兼容协议走 GET {base_url}/models；Anthropic 兼容协议走 GET {base_url}/v1/models。
    pub async fn fetch_remote_models(
        &self,
        protocol: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<Vec<shared_types::RemoteModel>, String> {
        let trimmed = base_url.trim_end_matches('/');
        let (url, is_anthropic) = if protocol == "anthropic" {
            if trimmed.ends_with("/models") {
                (trimmed.to_string(), true)
            } else if trimmed.ends_with("/v1") {
                (format!("{}/models", trimmed), true)
            } else {
                (format!("{}/v1/models", trimmed), true)
            }
        } else if trimmed.ends_with("/models") {
            (trimmed.to_string(), false)
        } else {
            (format!("{}/models", trimmed), false)
        };

        let mut req = self.client.get(&url);
        req = if is_anthropic {
            req.header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
        } else {
            req.header("Authorization", format!("Bearer {}", api_key))
        };

        let res = req
            .send()
            .await
            .map_err(|e| format!("拉取模型列表失败（网络错误）: {}", e))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            return Err(format!("拉取模型列表失败 ({}): {}", status, err_text));
        }

        #[derive(serde::Deserialize)]
        struct RawModel {
            id: String,
            #[serde(default)]
            owned_by: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct ModelList {
            #[serde(default)]
            data: Vec<RawModel>,
        }

        let parsed: ModelList = res
            .json()
            .await
            .map_err(|e| format!("解析模型列表响应失败: {}", e))?;

        let mut models: Vec<shared_types::RemoteModel> = parsed
            .data
            .into_iter()
            .map(|m| shared_types::RemoteModel {
                id: m.id,
                owned_by: m.owned_by.unwrap_or_default(),
            })
            .collect();

        if models.is_empty() {
            return Err("该接口未返回任何可用模型，请确认 Base URL 与协议是否匹配".to_string());
        }

        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    pub async fn call_stream(
        &self,
        app: tauri::AppHandle,
        stream_event_name: &str,
        profile: &LlmProfile,
        api_key: &str,
        system_prompt: Option<String>,
        messages: Vec<ChatMessage>,
    ) -> Result<String, String> {
        let event_name = stream_event_name.to_string();
        let app_handle = Arc::new(app);

        // 40ms micro-batching buffer（与 tail_flush 共享同一缓冲区，
        // 以便请求结束后补发未满 40ms 窗口的残余 token）
        let buffer = Arc::new(std::sync::Mutex::new(String::new()));
        let mut last_flush = Instant::now();

        let chunk_callback = {
            let app_handle = Arc::clone(&app_handle);
            let event_name = event_name.clone();
            let buffer = Arc::clone(&buffer);
            move |token: String| {
                let mut buf = match buffer.lock() {
                    Ok(b) => b,
                    Err(poisoned) => poisoned.into_inner(),
                };
                buf.push_str(&token);
                if last_flush.elapsed() >= Duration::from_millis(40) {
                    let _ = app_handle.emit(&event_name, buf.as_str());
                    buf.clear();
                    last_flush = Instant::now();
                }
            }
        };

        // 生成参数：temperature / 最大输出长度 / 是否流式（PRD §5.1 P0）
        let params = generation_params_of(profile);
        let temperature = params.temperature;
        let max_tokens = params.max_tokens;
        let stream = params.stream.unwrap_or(true);

        let result = match profile.protocol.as_str() {
            "anthropic" => {
                let anthropic_msgs = messages
                    .into_iter()
                    .map(|m| AnthropicMessage {
                        role: m.role,
                        content: m.content,
                    })
                    .collect();
                anthropic::stream_anthropic(
                    &self.client,
                    &profile.base_url,
                    api_key,
                    &profile.model,
                    system_prompt,
                    anthropic_msgs,
                    temperature,
                    max_tokens,
                    stream,
                    chunk_callback,
                )
                .await
            }
            _ => {
                let mut full_messages = Vec::new();
                if let Some(sys) = system_prompt {
                    full_messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: sys,
                    });
                }
                full_messages.extend(messages);

                openai::stream_openai(
                    &self.client,
                    &profile.base_url,
                    api_key,
                    &profile.model,
                    full_messages,
                    temperature,
                    max_tokens,
                    stream,
                    chunk_callback,
                )
                .await
            }
        };

        // 补发缓冲余量，避免最后一次未满 40ms 的 token 被丢弃
        if let Ok(mut buf) = buffer.lock() {
            if !buf.is_empty() {
                let _ = app_handle.emit(&event_name, buf.as_str());
                buf.clear();
            }
        }

        result
    }

    pub async fn call_sync(
        &self,
        profile: &LlmProfile,
        api_key: &str,
        system_prompt: Option<String>,
        messages: Vec<ChatMessage>,
    ) -> Result<String, String> {
        let noop = |_| {};
        // 同步调用（修饰 / 冲突检测 / 提炼）不关心「是否流式」开关，始终按流式收集，
        // 但 temperature 与最大输出长度沿用接口配置。
        let params = generation_params_of(profile);
        match profile.protocol.as_str() {
            "anthropic" => {
                let anthropic_msgs = messages
                    .into_iter()
                    .map(|m| AnthropicMessage {
                        role: m.role,
                        content: m.content,
                    })
                    .collect();
                anthropic::stream_anthropic(
                    &self.client,
                    &profile.base_url,
                    api_key,
                    &profile.model,
                    system_prompt,
                    anthropic_msgs,
                    params.temperature,
                    params.max_tokens,
                    true,
                    noop,
                )
                .await
            }
            _ => {
                let mut full_messages = Vec::new();
                if let Some(sys) = system_prompt {
                    full_messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: sys,
                    });
                }
                full_messages.extend(messages);

                openai::stream_openai(
                    &self.client,
                    &profile.base_url,
                    api_key,
                    &profile.model,
                    full_messages,
                    params.temperature,
                    params.max_tokens,
                    true,
                    noop,
                )
                .await
            }
        }
    }
}

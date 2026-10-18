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
        validate_base_url(&profile.base_url)?;
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
        validate_base_url(base_url)?;
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
        validate_base_url(&profile.base_url)?;
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
        validate_base_url(&profile.base_url)?;
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

/// 校验模型服务端点 URL 的协议与安全性。
///
/// 规则：
/// - 必须是合法的 HTTP 或 HTTPS URL；
/// - 若使用明文 `http://`：
///   必须是本机环回地址（`localhost`、`127.0.0.1`、`[::1]`、`0.0.0.0`）或局域网私有地址（`192.168.*`、`10.*`、`172.16~31.*`）；
///   对于公网远程地址，严禁使用明文 HTTP，以防 API Key 在公网裸奔传输。
pub fn validate_base_url(base_url: &str) -> Result<(), String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err("API Base URL 不能为空".to_string());
    }

    let lower = trimmed.to_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }

    if lower.starts_with("http://") {
        let without_proto = &trimmed[7..];
        let host_part = without_proto
            .split(['/', ':', '?', '#'])
            .next()
            .unwrap_or("")
            .trim();

        if is_local_or_private_host(host_part) {
            return Ok(());
        }

        return Err(format!(
            "安全拦截：远程模型服务「{}」使用了明文 HTTP 协议，可能导致 API 密钥在公网传输中泄露。公网远程接口请使用 HTTPS 协议，仅本地/局域网服务（如 localhost/127.0.0.1）允许使用 HTTP。",
            host_part
        ));
    }

    Err(format!(
        "无效的 Base URL 协议：「{}」，仅支持 https:// 或 http://",
        trimmed
    ))
}

fn is_local_or_private_host(host: &str) -> bool {
    let h = host.trim().to_lowercase();
    let clean_h = h.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(&h);
    if clean_h == "localhost" || clean_h == "127.0.0.1" || clean_h == "::1" || clean_h == "0.0.0.0" {
        return true;
    }

    // 局域网私有 IPv4
    if let Ok(ip) = clean_h.parse::<std::net::Ipv4Addr>() {
        let octets = ip.octets();
        if octets[0] == 127
            || octets[0] == 10
            || (octets[0] == 172 && (16..=31).contains(&octets[1]))
            || (octets[0] == 192 && octets[1] == 168)
        {
            return true;
        }
    }

    // 局域网私有 IPv6
    if let Ok(ip) = clean_h.parse::<std::net::Ipv6Addr>() {
        if ip.is_loopback() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_base_url_security_rules() {
        // HTTPS 远程接口放行
        assert!(validate_base_url("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url("https://api.deepseek.com").is_ok());
        assert!(validate_base_url("https://api.anthropic.com/v1").is_ok());

        // 本地开发与内网服务允许 HTTP
        assert!(validate_base_url("http://localhost:11434").is_ok());
        assert!(validate_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_base_url("http://0.0.0.0:8000").is_ok());
        assert!(validate_base_url("http://192.168.1.100:8000").is_ok());
        assert!(validate_base_url("http://10.0.1.20:8080/v1").is_ok());
        assert!(validate_base_url("http://172.16.0.5:8000").is_ok());

        // 公网明文 HTTP 强制拦截保护密钥
        assert!(validate_base_url("http://api.openai.com/v1").is_err());
        assert!(validate_base_url("http://api.deepseek.com").is_err());
        assert!(validate_base_url("http://47.98.12.34:8000").is_err());

        // 非法协议拦截
        assert!(validate_base_url("ftp://127.0.0.1").is_err());
        assert!(validate_base_url("").is_err());
    }
}

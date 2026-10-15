use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    delta: Option<AnthropicStreamDelta>,
}

pub async fn stream_anthropic(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    system_prompt: Option<String>,
    messages: Vec<AnthropicMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    // 生成参数中的「是否流式」开关；关闭时走一次性非流式收集，再回调整段文本。
    stream: bool,
    mut on_chunk: impl FnMut(String),
) -> Result<String, String> {
    let url = if base_url.ends_with("/messages") {
        base_url.to_string()
    } else {
        format!("{}/v1/messages", base_url.trim_end_matches('/'))
    };

    if !stream {
        let text = call_anthropic_once(
            client,
            &url,
            api_key,
            model,
            system_prompt,
            messages,
            temperature,
            max_tokens,
        )
        .await?;
        on_chunk(text.clone());
        return Ok(text);
    }

    let req_body = AnthropicRequest {
        model: model.to_string(),
        max_tokens: max_tokens.unwrap_or(4096),
        system: system_prompt,
        messages,
        temperature,
        stream: true,
    };

    let mut res = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Anthropic API error ({}): {}", status, err_text));
    }

    let mut full_content = String::new();
    let mut buffer = String::new();

    while let Some(item) = res.chunk().await.map_err(|e| e.to_string())? {
        let chunk_str = String::from_utf8_lossy(&item);
        buffer.push_str(&chunk_str);

        while let Some(idx) = buffer.find('\n') {
            let line = buffer[..idx].trim().to_string();
            buffer.drain(..=idx);

            if line.starts_with("data: ") {
                let data = &line["data: ".len()..];
                if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                    if event.event_type.as_deref() == Some("content_block_delta") {
                        if let Some(delta) = event.delta {
                            if let Some(text) = delta.text {
                                full_content.push_str(&text);
                                on_chunk(text);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(full_content)
}

/// 非流式一次性补全，供「是否流式 = 关闭」时使用。
#[allow(clippy::too_many_arguments)]
async fn call_anthropic_once(
    client: &Client,
    url: &str,
    api_key: &str,
    model: &str,
    system_prompt: Option<String>,
    messages: Vec<AnthropicMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct AnthropicContentBlock {
        #[serde(default)]
        text: String,
    }
    #[derive(Deserialize)]
    struct AnthropicResponse {
        content: Option<Vec<AnthropicContentBlock>>,
    }

    let req_body = AnthropicRequest {
        model: model.to_string(),
        max_tokens: max_tokens.unwrap_or(4096),
        system: system_prompt,
        messages,
        temperature,
        stream: false,
    };

    let res = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Anthropic API error ({}): {}", status, err_text));
    }

    let parsed: AnthropicResponse = res
        .json()
        .await
        .map_err(|e| format!("解析补全响应失败: {}", e))?;

    Ok(parsed
        .content
        .unwrap_or_default()
        .into_iter()
        .map(|b| b.text)
        .collect::<Vec<_>>()
        .join(""))
}

pub async fn test_anthropic_connection(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
) -> Result<(), String> {
    let url = if base_url.ends_with("/messages") {
        base_url.to_string()
    } else {
        format!("{}/v1/messages", base_url.trim_end_matches('/'))
    };

    let req_body = AnthropicRequest {
        model: model.to_string(),
        max_tokens: 10,
        system: None,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }],
        temperature: Some(0.1),
        stream: false,
    };

    let res = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Connection test failed: {}", e))?;

    if res.status().is_success() {
        Ok(())
    } else {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        Err(format!("Status {}: {}", status, err_text))
    }
}

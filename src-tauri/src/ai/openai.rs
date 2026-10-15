use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Option<Vec<StreamChoice>>,
}

pub async fn stream_openai(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    // 生成参数中的「是否流式」开关；关闭时走一次性非流式收集，再回调整段文本。
    stream: bool,
    mut on_chunk: impl FnMut(String),
) -> Result<String, String> {
    let url = if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{}/chat/completions", base_url.trim_end_matches('/'))
    };

    if !stream {
        let text = call_openai_once(
            client,
            &url,
            api_key,
            model,
            messages,
            temperature,
            max_tokens,
        )
        .await?;
        on_chunk(text.clone());
        return Ok(text);
    }

    let req_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        temperature,
        max_tokens,
        stream: true,
    };

    let mut res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error ({}): {}", status, err_text));
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
                if data == "[DONE]" {
                    break;
                }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(choices) = chunk.choices {
                        if let Some(first) = choices.into_iter().next() {
                            if let Some(delta) = first.delta.content {
                                full_content.push_str(&delta);
                                on_chunk(delta);
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
async fn call_openai_once(
    client: &Client,
    url: &str,
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct CompletionChoice {
        message: CompletionMessage,
    }
    #[derive(Deserialize)]
    struct CompletionMessage {
        #[serde(default)]
        content: String,
    }
    #[derive(Deserialize)]
    struct CompletionResponse {
        choices: Option<Vec<CompletionChoice>>,
    }

    let req_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        temperature,
        max_tokens,
        stream: false,
    };

    let res = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error ({}): {}", status, err_text));
    }

    let parsed: CompletionResponse = res
        .json()
        .await
        .map_err(|e| format!("解析补全响应失败: {}", e))?;

    Ok(parsed
        .choices
        .and_then(|c| c.into_iter().next())
        .map(|c| c.message.content)
        .unwrap_or_default())
}

pub async fn test_openai_connection(
    client: &Client,
    base_url: &str,
    api_key: &str,
    model: &str,
) -> Result<(), String> {
    let url = if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{}/chat/completions", base_url.trim_end_matches('/'))
    };

    let req_body = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }],
        temperature: Some(0.1),
        max_tokens: None,
        stream: false,
    };

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
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

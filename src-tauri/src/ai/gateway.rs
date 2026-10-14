use crate::ai::anthropic::{self, AnthropicMessage};
use crate::ai::openai::{self, ChatMessage};
use reqwest::Client;
use shared_types::LlmProfile;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;

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

        // 40ms micro-batching buffer
        let mut last_flush = Instant::now();
        let mut batch_buffer = String::new();

        let chunk_callback = move |token: String| {
            batch_buffer.push_str(&token);
            if last_flush.elapsed() >= Duration::from_millis(40) {
                let _ = app_handle.emit(&event_name, &batch_buffer);
                batch_buffer.clear();
                last_flush = Instant::now();
            }
        };

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
                    None,
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
                    None,
                    chunk_callback,
                )
                .await
            }
        };

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
                    None,
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
                    None,
                    noop,
                )
                .await
            }
        }
    }
}

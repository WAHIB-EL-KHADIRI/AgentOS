use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use tracing::debug;

use crate::provider::{LLMProvider, LLMProviderError, LLMProviderResult};
use crate::types::*;

/// Anthropic Claude LLM provider.
///
/// Connects to the Anthropic Messages API for Claude models.
///
/// # Environment Variables
///
/// - `ANTHROPIC_API_KEY` - required
/// - `ANTHROPIC_MODEL` - defaults to `claude-sonnet-4-20250514`
pub struct AnthropicProvider {
    api_key: String,
    model: String,
    base_url: String,
    api_version: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicProvider")
            .field("model", &self.model)
            .field("api_version", &self.api_version)
            .field("base_url", &self.base_url)
            .field("api_key", &"***")
            .finish()
    }
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_version: "2023-06-01".into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("valid reqwest client"),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = version.into();
        self
    }

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        let model =
            std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".into());
        Some(Self::new(api_key, model))
    }

    fn build_request_body(&self, request: &ChatCompletionRequest) -> serde_json::Value {
        // Anthropic separates system prompt from messages
        let system_prompts: Vec<String> = request
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone())
            .collect();

        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| {
                let role = match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                    _ => "user",
                };
                let mut msg = json!({
                    "role": role,
                    "content": m.content,
                });
                if let Some(ref tool_call_id) = m.tool_call_id {
                    msg["tool_use_id"] = json!(tool_call_id);
                }
                msg
            })
            .collect();

        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        if !system_prompts.is_empty() {
            body["system"] = json!(system_prompts.join("\n\n"));
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = json!(top_p);
        }
        if !request.tools.is_empty() {
            body["tools"] = json!(request
                .tools
                .iter()
                .map(|t| {
                    let tool = json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters.clone().unwrap_or(json!({
                            "type": "object",
                            "properties": {}
                        })),
                    });
                    tool
                })
                .collect::<Vec<_>>());
        }
        if request.stream {
            body["stream"] = json!(true);
        }

        body
    }

    fn parse_response(
        &self,
        response_body: serde_json::Value,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        let content_blocks = response_body["content"]
            .as_array()
            .map(|blocks| {
                let text: String = blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                let tool_calls: Vec<ToolCall> = blocks
                    .iter()
                    .filter_map(|b| {
                        if b.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                            return None;
                        }
                        Some(ToolCall {
                            id: b.get("id")?.as_str()?.to_string(),
                            name: b.get("name")?.as_str()?.to_string(),
                            arguments: b.get("input").cloned().unwrap_or(json!({})),
                        })
                    })
                    .collect();
                (text, tool_calls)
            })
            .unwrap_or_default();

        Ok(ChatCompletionResponse {
            id: response_body["id"].as_str().unwrap_or("").to_string(),
            model: response_body["model"].as_str().unwrap_or("").to_string(),
            content: content_blocks.0,
            tool_calls: content_blocks.1,
            finish_reason: response_body["stop_reason"]
                .as_str()
                .unwrap_or("end_turn")
                .to_string(),
            usage: response_body.get("usage").map(|u| {
                let input = u["input_tokens"].as_u64().unwrap_or(0) as u32;
                let output = u["output_tokens"].as_u64().unwrap_or(0) as u32;
                Usage {
                    prompt_tokens: input,
                    completion_tokens: output,
                    total_tokens: input + output,
                }
            }),
        })
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn chat(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        let body = self.build_request_body(&request);

        debug!(
            provider = "anthropic",
            model = %request.model,
            messages = request.messages.len(),
            "sending chat request"
        );

        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("anthropic-beta", "tools-2024-04-04")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMProviderError::RequestFailed(e.to_string()))?;

        let status = response.status();
        let response_body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LLMProviderError::ApiError(format!("parse failed: {e}")))?;

        if !status.is_success() {
            let error_msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            return if status.as_u16() == 429 {
                Err(LLMProviderError::RateLimited(
                    response_body["error"]["retry_after"].as_u64().unwrap_or(30),
                ))
            } else if status.as_u16() == 401 {
                Err(LLMProviderError::AuthError(error_msg))
            } else {
                Err(LLMProviderError::ApiError(format!(
                    "HTTP {}: {}",
                    status, error_msg
                )))
            };
        }

        self.parse_response(response_body)
    }

    async fn chat_stream(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<
        Box<dyn futures::Stream<Item = LLMProviderResult<ChatCompletionChunk>> + Send + Unpin>,
    > {
        let mut body = self.build_request_body(&request);
        body["stream"] = json!(true);

        debug!(
            provider = "anthropic",
            model = %request.model,
            "starting streaming chat"
        );

        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("anthropic-beta", "tools-2024-04-04")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMProviderError::RequestFailed(e.to_string()))?;

        let stream = response
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(chunk) => {
                    let text = String::from_utf8_lossy(&chunk);
                    // Parse SSE events from Anthropic streaming
                    let mut content = String::new();
                    for line in text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                break;
                            }
                            if let Ok(json_data) = serde_json::from_str::<serde_json::Value>(data) {
                                if json_data["type"] == "content_block_delta" {
                                    if let Some(delta) = json_data["delta"]["text"].as_str() {
                                        content.push_str(delta);
                                    }
                                }
                            }
                        }
                    }
                    Ok(ChatCompletionChunk {
                        id: String::new(),
                        model: request.model.clone(),
                        content: if content.is_empty() {
                            None
                        } else {
                            Some(content)
                        },
                        tool_calls: Vec::new(),
                        finish_reason: None,
                    })
                }
                Err(e) => Err(LLMProviderError::StreamError(e.to_string())),
            });

        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_body() {
        let provider = AnthropicProvider::new("test-key", "claude-sonnet-4-20250514");
        let request = ChatCompletionRequest::new(
            "claude-sonnet-4-20250514",
            vec![Message::system("You are helpful."), Message::user("Hello!")],
        );
        let body = provider.build_request_body(&request);
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["system"], "You are helpful.");
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = AnthropicProvider::new("test-key", "claude-sonnet-4-20250514");
        let mut request = ChatCompletionRequest::new(
            "claude-sonnet-4-20250514",
            vec![Message::user("What is the weather?")],
        );
        request
            .tools
            .push(ToolDefinition::new("get_weather", "Get weather"));
        let body = provider.build_request_body(&request);
        assert!(body.get("tools").is_some());
        assert_eq!(body["tools"][0]["name"], "get_weather");
    }

    #[test]
    fn test_parse_response() {
        let provider = AnthropicProvider::new("test-key", "claude-sonnet-4-20250514");
        let response = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {"type": "text", "text": "Hello! How can I help?"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });
        let parsed = provider.parse_response(response).unwrap();
        assert_eq!(parsed.content, "Hello! How can I help?");
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn test_parse_response_with_tool_calls() {
        let provider = AnthropicProvider::new("test-key", "claude-sonnet-4-20250514");
        let response = json!({
            "id": "msg_456",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": "tu_123",
                    "name": "get_weather",
                    "input": {"location": "Paris"}
                }
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 30}
        });
        let parsed = provider.parse_response(response).unwrap();
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "get_weather");
        assert_eq!(parsed.tool_calls[0].arguments["location"], "Paris");
    }

    #[test]
    fn test_provider_from_env_missing() {
        let orig = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");
        assert!(AnthropicProvider::from_env().is_none());
        if let Some(val) = orig {
            std::env::set_var("ANTHROPIC_API_KEY", val);
        }
    }

    #[test]
    fn test_is_configured() {
        let provider = AnthropicProvider::new("key", "claude-3-opus");
        assert!(provider.is_configured());

        let provider = AnthropicProvider::new("", "claude-3-opus");
        assert!(!provider.is_configured());
    }
}

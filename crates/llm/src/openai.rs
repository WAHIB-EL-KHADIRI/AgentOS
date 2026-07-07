use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::provider::{LLMProvider, LLMProviderError, LLMProviderResult};
use crate::types::*;

/// OpenAI-compatible LLM provider.
pub struct OpenAIProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for OpenAIProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIProvider")
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &"***")
            .finish()
    }
}

impl OpenAIProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.openai.com/v1".into(),
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

    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".into());
        Some(Self::new(api_key, model))
    }

    fn build_request_body(&self, request: &ChatCompletionRequest) -> serde_json::Value {
        let mut body = json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                let mut msg = json!({
                    "role": m.role,
                    "content": m.content,
                });
                if let Some(ref name) = m.name {
                    msg["name"] = json!(name);
                }
                if let Some(ref tool_call_id) = m.tool_call_id {
                    msg["tool_call_id"] = json!(tool_call_id);
                }
                msg
            }).collect::<Vec<_>>(),
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if let Some(top_p) = request.top_p {
            body["top_p"] = json!(top_p);
        }
        if !request.tools.is_empty() {
            body["tools"] = json!(request
                .tools
                .iter()
                .map(|t| {
                    let mut tool = json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                        }
                    });
                    if let Some(ref params) = t.parameters {
                        tool["function"]["parameters"] = params.clone();
                    }
                    tool
                })
                .collect::<Vec<_>>());
        }
        if request.stream {
            body["stream"] = json!(true);
            body["stream_options"] = json!({"include_usage": true});
        }

        body
    }

    fn parse_response(
        &self,
        response_body: serde_json::Value,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        let choice = response_body["choices"][0]
            .as_object()
            .ok_or_else(|| LLMProviderError::ApiError("no choices in response".into()))?;

        let content = choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .unwrap_or("stop")
            .to_string();

        let tool_calls = choice
            .get("message")
            .and_then(|m| m.get("tool_calls"))
            .and_then(|tc| tc.as_array())
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc.get("id")?.as_str()?.to_string(),
                            name: tc.get("function")?.get("name")?.as_str()?.to_string(),
                            arguments: tc
                                .get("function")?
                                .get("arguments")
                                .and_then(|a| serde_json::from_str(a.as_str()?).ok())
                                .unwrap_or(json!({})),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let usage = response_body.get("usage").map(|u| Usage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        Ok(ChatCompletionResponse {
            id: response_body["id"].as_str().unwrap_or("").to_string(),
            model: response_body["model"].as_str().unwrap_or("").to_string(),
            content,
            tool_calls,
            finish_reason,
            usage,
        })
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAI
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

        debug!(model = %request.model, messages = %request.messages.len(), "OpenAI chat request");

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMProviderError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        let response_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LLMProviderError::RequestFailed(format!("parse error: {e}")))?;

        if status == 429 {
            let retry_after = response_body["retry_after"].as_u64().unwrap_or(5);
            warn!("OpenAI rate limited, retry after {retry_after}s");
            return Err(LLMProviderError::RateLimited(retry_after));
        }

        if !status.is_success() {
            let error_msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            if status == 401 {
                return Err(LLMProviderError::AuthError(error_msg));
            }
            return Err(LLMProviderError::ApiError(format!(
                "HTTP {status}: {error_msg}"
            )));
        }

        info!(model = %request.model, "OpenAI chat completed");
        self.parse_response(response_body)
    }

    async fn chat_stream(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<
        Box<dyn futures::Stream<Item = LLMProviderResult<ChatCompletionChunk>> + Send + Unpin>,
    > {
        let mut streaming_req = request.clone();
        streaming_req.stream = true;
        let body = self.build_request_body(&streaming_req);

        debug!(model = %request.model, "OpenAI streaming request");

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMProviderError::RequestFailed(e.to_string()))?;

        let status = resp.status();
        if status == 429 {
            return Err(LLMProviderError::RateLimited(5));
        }
        if !status.is_success() {
            let response_body: serde_json::Value = resp.json().await.unwrap_or(json!({}));
            let error_msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(LLMProviderError::ApiError(format!(
                "HTTP {status}: {error_msg}"
            )));
        }

        let stream = resp.bytes_stream().filter_map(move |result| {
            let bytes = match result {
                Ok(b) => b,
                Err(e) => {
                    return futures::future::ready(Some(Err(LLMProviderError::StreamError(
                        e.to_string(),
                    ))))
                }
            };

            let text = String::from_utf8_lossy(&bytes);
            let mut chunks: Vec<LLMProviderResult<ChatCompletionChunk>> = Vec::new();

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "data: [DONE]" {
                    break;
                }
                if let Some(json_str) = line.strip_prefix("data: ") {
                    match serde_json::from_str::<serde_json::Value>(json_str) {
                        Ok(val) => {
                            let chunk = parse_stream_chunk(val);
                            chunks.push(chunk);
                        }
                        Err(e) => {
                            error!("Failed to parse SSE chunk: {e}");
                        }
                    }
                }
            }

            if chunks.is_empty() {
                futures::future::ready(None)
            } else {
                let mut iter = chunks.into_iter();
                let first = iter.next().expect(
                    "chunks iterator should yield at least one element when chunks is non-empty",
                );
                futures::future::ready(Some(first))
            }
        });

        Ok(Box::new(stream))
    }
}

fn parse_stream_chunk(chunk: serde_json::Value) -> LLMProviderResult<ChatCompletionChunk> {
    let id = chunk["id"].as_str().unwrap_or("").to_string();
    let model = chunk["model"].as_str().unwrap_or("").to_string();

    let choice = chunk["choices"][0].as_object().cloned().unwrap_or_default();
    let content = choice
        .get("delta")
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());

    let tool_calls = choice
        .get("delta")
        .and_then(|d| d.get("tool_calls"))
        .and_then(|tc| tc.as_array())
        .map(|calls| {
            calls
                .iter()
                .filter_map(|tc| {
                    Some(ToolCall {
                        id: tc.get("id")?.as_str()?.to_string(),
                        name: tc.get("function")?.get("name")?.as_str()?.to_string(),
                        arguments: tc
                            .get("function")?
                            .get("arguments")
                            .and_then(|a| serde_json::from_str(a.as_str()?).ok())
                            .unwrap_or(json!({})),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ChatCompletionChunk {
        id,
        model,
        content,
        tool_calls,
        finish_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_body() {
        let provider = OpenAIProvider::new("sk-test", "gpt-4o");
        let req = ChatCompletionRequest::new(
            "gpt-4o",
            vec![Message::system("You are helpful"), Message::user("Hello!")],
        );
        let body = provider.build_request_body(&req);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "You are helpful");
        assert_eq!(body["messages"][1]["role"], "user");
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = OpenAIProvider::new("sk-test", "gpt-4o");
        let mut req = ChatCompletionRequest::new("gpt-4o", vec![Message::user("Hi")]);
        req.tools.push(ToolDefinition::new(
            "get_weather",
            "Get the weather for a location",
        ));
        let body = provider.build_request_body(&req);
        assert!(body.get("tools").is_some());
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_build_request_body_streaming() {
        let provider = OpenAIProvider::new("sk-test", "gpt-4o");
        let mut req = ChatCompletionRequest::new("gpt-4o", vec![Message::user("Hi")]);
        req.stream = true;
        let body = provider.build_request_body(&req);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_parse_response() {
        let provider = OpenAIProvider::new("sk-test", "gpt-4o");
        let json = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help?"
                },
                "finish_reason": "stop",
                "index": 0
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let resp = provider.parse_response(json).unwrap();
        assert_eq!(resp.content, "Hello! How can I help?");
        assert_eq!(resp.finish_reason, "stop");
        assert_eq!(resp.usage.unwrap().total_tokens, 15);
    }

    #[test]
    fn test_parse_response_with_tool_calls() {
        let provider = OpenAIProvider::new("sk-test", "gpt-4o");
        let json = json!({
            "id": "chatcmpl-456",
            "model": "gpt-4o",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"Paris\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls",
                "index": 0
            }]
        });
        let resp = provider.parse_response(json).unwrap();
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "get_weather");
        assert_eq!(resp.tool_calls[0].arguments["location"], "Paris");
        assert_eq!(resp.finish_reason, "tool_calls");
    }

    #[test]
    fn test_provider_from_env_missing() {
        let orig = std::env::var("OPENAI_API_KEY").ok();
        std::env::remove_var("OPENAI_API_KEY");
        assert!(OpenAIProvider::from_env().is_none());
        if let Some(val) = orig {
            std::env::set_var("OPENAI_API_KEY", val);
        }
    }

    #[test]
    fn test_parse_stream_chunk() {
        let json = json!({
            "id": "chatcmpl-789",
            "model": "gpt-4o",
            "choices": [{
                "delta": {"content": "Hello"},
                "finish_reason": null,
                "index": 0
            }]
        });
        let chunk = parse_stream_chunk(json).unwrap();
        assert_eq!(chunk.content.as_deref(), Some("Hello"));
        assert!(chunk.finish_reason.is_none());
    }
}

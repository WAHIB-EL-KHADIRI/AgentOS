use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use tracing::debug;

use crate::provider::{LLMProvider, LLMProviderError, LLMProviderResult};
use crate::types::*;

/// Ollama local LLM provider.
///
/// Connects to a local Ollama instance for running models locally.
///
/// # Environment Variables
///
/// - `OLLAMA_BASE_URL` - defaults to `http://localhost:11434`
/// - `OLLAMA_MODEL` - defaults to `llama3.2`
pub struct OllamaProvider {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for OllamaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaProvider")
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl OllamaProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            base_url: "http://localhost:11434".into(),
            model: model.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("valid reqwest client"),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn from_env() -> Option<Self> {
        let base_url =
            std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".into());
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".into());
        Some(Self::new(model).with_base_url(base_url))
    }

    fn build_request_body(&self, request: &ChatCompletionRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                };
                json!({
                    "role": role,
                    "content": m.content,
                })
            })
            .collect();

        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "stream": request.stream,
        });

        if let Some(temp) = request.temperature {
            body["options"] = json!({"temperature": temp});
        }

        body
    }

    fn parse_response(
        &self,
        response_body: serde_json::Value,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        let message = response_body["message"]
            .as_object()
            .ok_or_else(|| LLMProviderError::ApiError("no message in response".into()))?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();

        Ok(ChatCompletionResponse {
            id: response_body["created_at"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            model: response_body["model"].as_str().unwrap_or("").to_string(),
            content,
            tool_calls: Vec::new(),
            finish_reason: if response_body["done"].as_bool().unwrap_or(false) {
                "stop"
            } else {
                "in_progress"
            }
            .to_string(),
            usage: response_body.get("prompt_eval_count").map(|_| Usage {
                prompt_tokens: response_body["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
                completion_tokens: response_body["eval_count"].as_u64().unwrap_or(0) as u32,
                total_tokens: (response_body["prompt_eval_count"].as_u64().unwrap_or(0)
                    + response_body["eval_count"].as_u64().unwrap_or(0))
                    as u32,
            }),
        })
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Custom
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn is_configured(&self) -> bool {
        true // Ollama can always be tried
    }

    async fn chat(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        let body = self.build_request_body(&request);

        debug!(
            provider = "ollama",
            model = %request.model,
            "sending chat request to local Ollama"
        );

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
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
            return Err(LLMProviderError::ApiError(format!(
                "Ollama HTTP {}: {:?}",
                status,
                response_body.get("error")
            )));
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
            provider = "ollama",
            model = %request.model,
            "starting streaming chat with local Ollama"
        );

        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LLMProviderError::RequestFailed(e.to_string()))?;

        let stream = response
            .bytes_stream()
            .map(move |chunk_result| match chunk_result {
                Ok(chunk) => {
                    let text = String::from_utf8_lossy(&chunk);
                    let mut content = String::new();
                    for line in text.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Ok(json_data) = serde_json::from_str::<serde_json::Value>(line) {
                            if let Some(delta) = json_data["message"]["content"].as_str() {
                                content.push_str(delta);
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
        let provider = OllamaProvider::new("llama3.2");
        let request = ChatCompletionRequest::new(
            "llama3.2",
            vec![Message::system("You are helpful."), Message::user("Hello!")],
        );
        let body = provider.build_request_body(&request);
        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn test_build_request_body_streaming() {
        let provider = OllamaProvider::new("llama3.2");
        let mut request = ChatCompletionRequest::new("llama3.2", vec![Message::user("Hi")]);
        request.stream = true;
        let body = provider.build_request_body(&request);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn test_parse_response() {
        let provider = OllamaProvider::new("llama3.2");
        let response = json!({
            "model": "llama3.2",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help?"
            },
            "done": true,
            "prompt_eval_count": 10,
            "eval_count": 5
        });
        let parsed = provider.parse_response(response).unwrap();
        assert_eq!(parsed.content, "Hello! How can I help?");
        assert_eq!(parsed.finish_reason, "stop");
    }

    #[test]
    fn test_parse_response_no_usage() {
        let provider = OllamaProvider::new("llama3.2");
        let response = json!({
            "model": "llama3.2",
            "message": {"role": "assistant", "content": "Hi"}
        });
        let parsed = provider.parse_response(response).unwrap();
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn test_from_env_defaults() {
        let orig_url = std::env::var("OLLAMA_BASE_URL").ok();
        let orig_model = std::env::var("OLLAMA_MODEL").ok();
        std::env::remove_var("OLLAMA_BASE_URL");
        std::env::remove_var("OLLAMA_MODEL");
        let provider = OllamaProvider::from_env().unwrap();
        assert_eq!(provider.model, "llama3.2");
        assert_eq!(provider.base_url, "http://localhost:11434");
        if let Some(val) = orig_url {
            std::env::set_var("OLLAMA_BASE_URL", val);
        }
        if let Some(val) = orig_model {
            std::env::set_var("OLLAMA_MODEL", val);
        }
    }
}

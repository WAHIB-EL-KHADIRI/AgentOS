//! VCR-style replay provider: serves previously recorded LLM responses in
//! order instead of calling a real model. This is the foundation of
//! deterministic trace replay — a recorded agent session can be re-executed
//! byte-for-byte at the LLM boundary with no API key and no network.
//!
//! With a `fallback` provider configured, the replay provider turns into a
//! fork point: the recorded prefix plays back deterministically, then the
//! live provider continues from wherever the recording ends.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::provider::{LLMProvider, LLMProviderError, LLMProviderResult};
use crate::types::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ProviderKind, ToolCall,
};

/// A recorded LLM response, serializable for journals on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedResponse {
    pub model: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
}

impl RecordedResponse {
    pub fn from_response(response: &ChatCompletionResponse) -> Self {
        Self {
            model: response.model.clone(),
            content: response.content.clone(),
            tool_calls: response.tool_calls.clone(),
            finish_reason: response.finish_reason.clone(),
        }
    }

    fn into_response(self, id: String) -> ChatCompletionResponse {
        ChatCompletionResponse {
            id,
            model: self.model,
            content: self.content,
            tool_calls: self.tool_calls,
            finish_reason: self.finish_reason,
            usage: None,
        }
    }
}

/// Serves recorded responses in order; optionally falls back to a live
/// provider once the recording is exhausted (fork semantics).
pub struct ReplayProvider {
    responses: Mutex<VecDeque<RecordedResponse>>,
    fallback: Option<Arc<dyn LLMProvider>>,
    served: Mutex<usize>,
    /// Model name to impersonate. Replayed requests must look exactly like
    /// the recorded ones, so this should be the original request model.
    model: String,
}

impl std::fmt::Debug for ReplayProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayProvider")
            .field("remaining", &self.responses.lock().unwrap().len())
            .field("has_fallback", &self.fallback.is_some())
            .finish()
    }
}

impl ReplayProvider {
    pub fn new(responses: Vec<RecordedResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            fallback: None,
            served: Mutex::new(0),
            model: "recorded".into(),
        }
    }

    /// Impersonate the original request model so replayed requests are
    /// byte-identical to the recorded ones.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        let model = model.into();
        if !model.trim().is_empty() {
            self.model = model;
        }
        self
    }

    /// After the recorded responses run out, continue with the live
    /// provider instead of failing. This is what makes `fork` possible.
    pub fn with_fallback(mut self, fallback: Arc<dyn LLMProvider>) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Number of recorded responses served so far.
    pub fn served(&self) -> usize {
        *self.served.lock().unwrap()
    }

    /// Number of recorded responses still queued.
    pub fn remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

#[async_trait]
impl LLMProvider for ReplayProvider {
    fn name(&self) -> &str {
        "replay"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Custom
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn is_configured(&self) -> bool {
        true
    }

    async fn chat(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<ChatCompletionResponse> {
        let next = self.responses.lock().unwrap().pop_front();
        match next {
            Some(recorded) => {
                let mut served = self.served.lock().unwrap();
                *served += 1;
                let id = format!("replay-{}", *served);
                Ok(recorded.into_response(id))
            }
            None => match &self.fallback {
                Some(live) => live.chat(request).await,
                None => Err(LLMProviderError::ApiError(
                    "recorded session exhausted and no live fallback provider is configured".into(),
                )),
            },
        }
    }

    async fn chat_stream(
        &self,
        _request: ChatCompletionRequest,
    ) -> LLMProviderResult<
        Box<dyn futures::Stream<Item = LLMProviderResult<ChatCompletionChunk>> + Send + Unpin>,
    > {
        Err(LLMProviderError::StreamError(
            "replay provider does not stream".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Message;

    fn recorded(content: &str) -> RecordedResponse {
        RecordedResponse {
            model: "test-model".into(),
            content: content.into(),
            tool_calls: Vec::new(),
            finish_reason: "stop".into(),
        }
    }

    fn request() -> ChatCompletionRequest {
        ChatCompletionRequest::new("any", vec![Message::user("hello")])
    }

    #[tokio::test]
    async fn test_replay_serves_responses_in_order() {
        let provider = ReplayProvider::new(vec![recorded("first"), recorded("second")]);
        assert_eq!(provider.remaining(), 2);

        let a = provider.chat(request()).await.unwrap();
        assert_eq!(a.content, "first");
        let b = provider.chat(request()).await.unwrap();
        assert_eq!(b.content, "second");
        assert_eq!(provider.served(), 2);
        assert_eq!(provider.remaining(), 0);
    }

    #[tokio::test]
    async fn test_exhausted_without_fallback_errors() {
        let provider = ReplayProvider::new(vec![recorded("only")]);
        provider.chat(request()).await.unwrap();

        let result = provider.chat(request()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("recorded session exhausted"));
    }

    #[tokio::test]
    async fn test_exhausted_with_fallback_continues_live() {
        #[derive(Debug)]
        struct LiveStub;

        #[async_trait]
        impl LLMProvider for LiveStub {
            fn name(&self) -> &str {
                "live-stub"
            }
            fn kind(&self) -> ProviderKind {
                ProviderKind::Custom
            }
            fn model(&self) -> &str {
                "live-model"
            }
            fn is_configured(&self) -> bool {
                true
            }
            async fn chat(
                &self,
                _request: ChatCompletionRequest,
            ) -> LLMProviderResult<ChatCompletionResponse> {
                Ok(ChatCompletionResponse {
                    id: "live-1".into(),
                    model: "live-model".into(),
                    content: "live continuation".into(),
                    tool_calls: Vec::new(),
                    finish_reason: "stop".into(),
                    usage: None,
                })
            }
            async fn chat_stream(
                &self,
                _request: ChatCompletionRequest,
            ) -> LLMProviderResult<
                Box<
                    dyn futures::Stream<Item = LLMProviderResult<ChatCompletionChunk>>
                        + Send
                        + Unpin,
                >,
            > {
                Err(LLMProviderError::StreamError("no stream".into()))
            }
        }

        let provider =
            ReplayProvider::new(vec![recorded("prefix")]).with_fallback(Arc::new(LiveStub));

        let a = provider.chat(request()).await.unwrap();
        assert_eq!(a.content, "prefix");
        let b = provider.chat(request()).await.unwrap();
        assert_eq!(b.content, "live continuation");
    }

    #[test]
    fn test_recorded_response_serde_roundtrip() {
        let original = RecordedResponse {
            model: "m".into(),
            content: "c".into(),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "lint".into(),
                arguments: serde_json::json!({"x": 1}),
            }],
            finish_reason: "tool_calls".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: RecordedResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "c");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "lint");
    }
}

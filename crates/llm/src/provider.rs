use async_trait::async_trait;

use crate::types::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ProviderKind,
};

pub type LLMProviderResult<T> = Result<T, LLMProviderError>;

#[derive(Debug, thiserror::Error)]
pub enum LLMProviderError {
    #[error("API request failed: {0}")]
    RequestFailed(String),

    #[error("API response error: {0}")]
    ApiError(String),

    #[error("rate limited: retry after {0}s")]
    RateLimited(u64),

    #[error("authentication failed: {0}")]
    AuthError(String),

    #[error("stream error: {0}")]
    StreamError(String),

    #[error("provider not configured: {0}")]
    NotConfigured(String),
}

/// Abstract LLM provider supporting chat completions (streaming + non-streaming).
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// The provider name (e.g. "openai", "anthropic").
    fn name(&self) -> &str;

    /// The provider kind.
    fn kind(&self) -> ProviderKind;

    /// The current model identifier.
    fn model(&self) -> &str;

    /// Non-streaming chat completion.
    async fn chat(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<ChatCompletionResponse>;

    /// Streaming chat completion – returns a stream of chunks.
    async fn chat_stream(
        &self,
        request: ChatCompletionRequest,
    ) -> LLMProviderResult<
        Box<dyn futures::Stream<Item = LLMProviderResult<ChatCompletionChunk>> + Send + Unpin>,
    >;

    /// Check if the provider is properly configured.
    fn is_configured(&self) -> bool;
}

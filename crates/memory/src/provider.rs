use async_trait::async_trait;

use crate::embedder::Embedder;

/// Provider kind for runtime selection of embedding backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Hashing,
    OpenAI,
    Custom,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Hashing => "hashing",
            ProviderKind::OpenAI => "openai",
            ProviderKind::Custom => "custom",
        }
    }
}

/// Production-ready async embedding provider trait.
///
/// Supports single and batch embedding, provider metadata, and configuration
/// for different backends (OpenAI, HuggingFace, local models, etc.).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a vector.
    async fn embed(&self, text: &str) -> Vec<f32>;

    /// Embed multiple texts in a single batch call.
    ///
    /// The default implementation calls `embed` for each text sequentially.
    /// Real providers should override this to use batched API calls.
    async fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await);
        }
        results
    }

    /// Return the dimensionality of the embedding vectors.
    fn dimensions(&self) -> usize;

    /// A human-readable name for the provider (e.g. "openai/text-embedding-3-small").
    fn provider_name(&self) -> &str {
        "unknown"
    }

    /// The kind of this provider for runtime configuration.
    fn kind(&self) -> ProviderKind {
        ProviderKind::Custom
    }
}

/// Configuration stub for OpenAI-compatible embedding APIs.
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub model: String,
    pub api_key: String,
    pub base_url: String,
    pub dimensions: usize,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            model: "text-embedding-3-small".into(),
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".into(),
            dimensions: 1536,
        }
    }
}

/// Wraps any sync `Embedder` into an async `EmbeddingProvider`.
pub struct EmbedderAdapter<E: Embedder> {
    inner: E,
}

impl<E: Embedder + Send + Sync> EmbedderAdapter<E> {
    pub fn new(embedder: E) -> Self {
        Self { inner: embedder }
    }

    pub fn into_inner(self) -> E {
        self.inner
    }
}

#[async_trait]
impl<E: Embedder + Send + Sync> EmbeddingProvider for EmbedderAdapter<E> {
    async fn embed(&self, text: &str) -> Vec<f32> {
        self.inner.embed(text)
    }

    fn dimensions(&self) -> usize {
        self.inner.dimensions()
    }

    fn provider_name(&self) -> &str {
        "embedder-adapter"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Hashing
    }
}

/// A registry of named embedding providers for runtime selection.
#[derive(Default)]
pub struct EmbeddingRegistry {
    providers: Vec<(String, Box<dyn EmbeddingProvider>)>,
}

impl std::fmt::Debug for EmbeddingRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddingRegistry")
            .field("providers", &self.list())
            .finish()
    }
}

impl EmbeddingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: &str, provider: Box<dyn EmbeddingProvider>) {
        self.providers.push((name.to_string(), provider));
    }

    pub fn get(&self, name: &str) -> Option<&dyn EmbeddingProvider> {
        self.providers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.as_ref())
    }

    pub fn list(&self) -> Vec<&str> {
        self.providers.iter().map(|(n, _)| n.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::HashingEmbedder;

    #[tokio::test]
    async fn test_embedder_adapter() {
        let hashing = HashingEmbedder::new(128);
        let adapter = EmbedderAdapter::new(hashing);

        let vec = adapter.embed("hello world").await;
        assert_eq!(vec.len(), 128);
        assert_eq!(adapter.dimensions(), 128);
    }

    #[tokio::test]
    async fn test_embedder_adapter_deterministic() {
        let hashing = HashingEmbedder::new(64);
        let adapter = EmbedderAdapter::new(hashing);

        let a = adapter.embed("same text").await;
        let b = adapter.embed("same text").await;
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn test_batch_embedding() {
        let hashing = HashingEmbedder::new(32);
        let adapter = EmbedderAdapter::new(hashing);

        let results = adapter.embed_batch(&["hello", "world", "embedding"]).await;
        assert_eq!(results.len(), 3);
        for vec in &results {
            assert_eq!(vec.len(), 32);
        }
    }

    #[test]
    fn test_provider_kind_as_str() {
        assert_eq!(ProviderKind::Hashing.as_str(), "hashing");
        assert_eq!(ProviderKind::OpenAI.as_str(), "openai");
        assert_eq!(ProviderKind::Custom.as_str(), "custom");
    }

    #[test]
    fn test_openai_config_default() {
        let config = OpenAIConfig::default();
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.dimensions, 1536);
    }

    #[tokio::test]
    async fn test_registry() {
        let mut registry = EmbeddingRegistry::new();

        let hashing = HashingEmbedder::new(64);
        let adapter = EmbedderAdapter::new(hashing);
        registry.register("default", Box::new(adapter));

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let provider = registry.get("default").unwrap();
        let vec = provider.embed("test").await;
        assert_eq!(vec.len(), 64);
    }

    #[test]
    fn test_registry_list() {
        let mut registry = EmbeddingRegistry::new();
        registry.register(
            "primary",
            Box::new(EmbedderAdapter::new(HashingEmbedder::new(32))),
        );
        registry.register(
            "secondary",
            Box::new(EmbedderAdapter::new(HashingEmbedder::new(64))),
        );

        let names = registry.list();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"primary"));
    }
}

#![forbid(unsafe_code)]

pub mod embedder;
pub mod provider;
pub mod store;

pub use embedder::{Embedder, HashingEmbedder};
pub use provider::{
    EmbedderAdapter, EmbeddingProvider, EmbeddingRegistry, OpenAIConfig, ProviderKind,
};
pub use store::{
    InMemoryStore, MemoryError, MemoryRecord, MemoryResult, MemoryStore, MemoryStoreConfig,
    SqliteMemoryStore,
};

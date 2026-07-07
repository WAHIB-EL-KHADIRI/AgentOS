#![forbid(unsafe_code)]

pub mod grpc;
pub mod in_memory;
pub mod websocket;

pub use grpc::{
    start_sse_server, GrpcBusClient, GrpcBusEndpoint, ProtoAgentEnvelope, PublishRequest,
    PublishResponse, SseEvent, SubscribeRequest,
};
pub use in_memory::InMemoryBus;

use std::fmt;

pub type BusResult<T> = Result<T, BusError>;

const MAX_AGENT_ID_LEN: usize = 256;
const MAX_TOPIC_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnvelope {
    pub id: String,
    pub source_agent_id: String,
    pub target_agent_id: String,
    pub topic: String,
    pub payload: Vec<u8>,
    pub timestamp_ms: u64,
}

impl AgentEnvelope {
    pub fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        topic: impl Into<String>,
        payload: Vec<u8>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let mut source = source.into();
        let mut target = target.into();
        let mut topic = topic.into();
        trim_or_truncate(&mut source, MAX_AGENT_ID_LEN);
        trim_or_truncate(&mut target, MAX_AGENT_ID_LEN);
        trim_or_truncate(&mut topic, MAX_TOPIC_LEN);
        Self {
            id: String::new(),
            source_agent_id: source,
            target_agent_id: target,
            topic,
            payload,
            timestamp_ms: now,
        }
    }
}

/// Trim whitespace and truncate to max_len. Removes null bytes to prevent
/// injection into storage backends that may be null-terminated.
fn trim_or_truncate(s: &mut String, max_len: usize) {
    let trimmed = s.replace('\0', "");
    let mut end = trimmed.trim().len();
    if end > max_len {
        end = max_len;
    }
    *s = trimmed[..end].to_string();
}

impl fmt::Display for AgentEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Envelope[id={}, source={}, target={}, topic={}, size={}]",
            self.id,
            self.source_agent_id,
            self.target_agent_id,
            self.topic,
            self.payload.len()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BusError {
    #[error("bus full, message rejected")]
    BusFull,

    #[error("agent '{0}' not connected to bus")]
    AgentNotConnected(String),

    #[error("bus closed")]
    BusClosed,

    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),
}

#[async_trait::async_trait]
pub trait AgentBusTrait: Send + Sync {
    async fn publish(&self, envelope: AgentEnvelope) -> BusResult<String>;
    async fn drain_for(&self, agent_id: &str) -> Vec<AgentEnvelope>;
    async fn subscribe(&self, agent_id: &str, topics: &[&str]);
    async fn unsubscribe(&self, agent_id: &str, topics: &[&str]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_creation() {
        let env = AgentEnvelope::new("alice", "bob", "test.topic", vec![1, 2, 3]);
        assert_eq!(env.source_agent_id, "alice");
        assert_eq!(env.target_agent_id, "bob");
        assert_eq!(env.topic, "test.topic");
        assert_eq!(env.payload, vec![1, 2, 3]);
        assert!(env.timestamp_ms > 0);
    }

    #[test]
    fn test_envelope_display() {
        let env = AgentEnvelope {
            id: "msg_1".into(),
            source_agent_id: "alice".into(),
            target_agent_id: "bob".into(),
            topic: "test".into(),
            payload: vec![0; 10],
            timestamp_ms: 1000,
        };
        let display = format!("{env}");
        assert!(display.contains("msg_1"));
        assert!(display.contains("alice"));
        assert!(display.contains("bob"));
    }
}

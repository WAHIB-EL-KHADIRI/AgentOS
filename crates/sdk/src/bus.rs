use agentos_bus::grpc::{GrpcBusClient, GrpcBusEndpoint};
use agentos_bus::{AgentBusTrait, AgentEnvelope};

use crate::error::SdkResult;

/// High-level client for interacting with the AgentOS message bus.
#[derive(Debug)]
pub struct BusClient {
    inner: GrpcBusClient,
    agent_id: String,
}

impl BusClient {
    pub fn new(addr: impl Into<String>, agent_id: impl Into<String>) -> Self {
        let endpoint = GrpcBusEndpoint::new(addr);
        let inner = GrpcBusClient::new(endpoint);
        Self {
            inner,
            agent_id: agent_id.into(),
        }
    }

    pub async fn publish(&self, topic: impl Into<String>, payload: Vec<u8>) -> SdkResult<String> {
        let envelope = AgentEnvelope::new(&self.agent_id, "broadcast", topic, payload);
        Ok(self.inner.publish(envelope).await?)
    }

    pub async fn publish_json<T: serde::Serialize>(
        &self,
        topic: impl Into<String>,
        value: &T,
    ) -> SdkResult<String> {
        let payload = serde_json::to_vec(value)?;
        self.publish(topic, payload).await
    }

    pub async fn subscribe(&self, topics: &[&str]) {
        self.inner.subscribe(&self.agent_id, topics).await;
    }

    pub async fn unsubscribe(&self, topics: &[&str]) {
        self.inner.unsubscribe(&self.agent_id, topics).await;
    }

    pub async fn drain(&self) -> Vec<AgentEnvelope> {
        self.inner.drain_for(&self.agent_id).await
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bus_client_creation() {
        let client = BusClient::new("127.0.0.1:9876", "test-agent");
        assert_eq!(client.agent_id(), "test-agent");
    }
}

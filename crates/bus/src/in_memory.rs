use std::collections::{HashMap, HashSet, VecDeque};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{AgentBusTrait, AgentEnvelope, BusError, BusResult};

#[derive(Debug)]
pub struct InMemoryBus {
    messages: Mutex<VecDeque<QueuedEnvelope>>,
    next_id: Mutex<u64>,
    subscriptions: Mutex<HashMap<String, Vec<String>>>,
}

#[derive(Debug, Clone)]
struct QueuedEnvelope {
    envelope: AgentEnvelope,
    recipients: HashSet<String>,
    delivered_to: HashSet<String>,
}

impl QueuedEnvelope {
    fn new(envelope: AgentEnvelope, recipients: HashSet<String>) -> Self {
        Self {
            envelope,
            recipients,
            delivered_to: HashSet::new(),
        }
    }

    fn is_for(&self, agent_id: &str) -> bool {
        self.recipients.contains(agent_id) && !self.delivered_to.contains(agent_id)
    }

    fn is_fully_delivered(&self) -> bool {
        self.recipients.is_subset(&self.delivered_to)
    }
}

impl Default for InMemoryBus {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBus {
    /// Create a local in-memory bus.
    ///
    /// Broadcast fanout is resolved at publish time: agents subscribed to the
    /// topic before `publish` each receive one copy, and targeted messages are
    /// delivered only to their explicit target.
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(VecDeque::new()),
            next_id: Mutex::new(0),
            subscriptions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn len(&self) -> usize {
        self.messages.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[async_trait]
impl AgentBusTrait for InMemoryBus {
    async fn publish(&self, mut envelope: AgentEnvelope) -> BusResult<String> {
        let mut next_id = self.next_id.lock().await;
        *next_id += 1;
        if envelope.id.is_empty() {
            envelope.id = format!("msg_{}", next_id);
        }
        let id = envelope.id.clone();
        let recipients = self.recipients_for(&envelope).await;
        let mut messages = self.messages.lock().await;

        if messages.len() > 10_000 {
            return Err(BusError::BusFull);
        }

        if !recipients.is_empty() {
            messages.push_back(QueuedEnvelope::new(envelope, recipients));
        }
        Ok(id)
    }

    async fn drain_for(&self, agent_id: &str) -> Vec<AgentEnvelope> {
        let mut messages = self.messages.lock().await;
        let mut matched = Vec::new();
        let mut remaining = VecDeque::new();

        while let Some(mut queued) = messages.pop_front() {
            if queued.is_for(agent_id) {
                queued.delivered_to.insert(agent_id.to_string());
                matched.push(queued.envelope.clone());
            }

            if !queued.is_fully_delivered() {
                remaining.push_back(queued);
            }
        }

        *messages = remaining;
        matched
    }

    async fn subscribe(&self, agent_id: &str, topics: &[&str]) {
        let mut subs = self.subscriptions.lock().await;
        subs.entry(agent_id.to_string())
            .or_default()
            .extend(topics.iter().map(|t| t.to_string()));
    }

    async fn unsubscribe(&self, agent_id: &str, topics: &[&str]) {
        let mut subs = self.subscriptions.lock().await;
        if let Some(topics_set) = subs.get_mut(agent_id) {
            topics_set.retain(|t| !topics.contains(&t.as_str()));
        }
    }
}

impl InMemoryBus {
    async fn recipients_for(&self, envelope: &AgentEnvelope) -> HashSet<String> {
        if envelope.target_agent_id != "*" {
            return HashSet::from([envelope.target_agent_id.clone()]);
        }

        let subscriptions = self.subscriptions.lock().await;
        subscriptions
            .iter()
            .filter(|(_, topics)| topics.contains(&envelope.topic))
            .map(|(agent_id, _)| agent_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_and_drain() {
        let bus = InMemoryBus::new();
        let env = AgentEnvelope::new("alice", "bob", "test", vec![1, 2, 3]);
        bus.publish(env).await.unwrap();

        let msgs = bus.drain_for("bob").await;
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].topic, "test");
    }

    #[tokio::test]
    async fn test_drain_only_targeted() {
        let bus = InMemoryBus::new();
        bus.publish(AgentEnvelope::new("alice", "bob", "test", vec![]))
            .await
            .unwrap();
        bus.publish(AgentEnvelope::new("alice", "charlie", "test", vec![]))
            .await
            .unwrap();

        let bob_msgs = bus.drain_for("bob").await;
        assert_eq!(bob_msgs.len(), 1);
        assert_eq!(bob_msgs[0].target_agent_id, "bob");
    }

    #[tokio::test]
    async fn test_subscription_based_routing() {
        let bus = InMemoryBus::new();
        bus.subscribe("charlie", &["broadcast"]).await;
        bus.publish(AgentEnvelope::new("alice", "*", "broadcast", vec![]))
            .await
            .unwrap();

        let msgs = bus.drain_for("charlie").await;
        assert_eq!(msgs.len(), 1);
    }

    #[tokio::test]
    async fn test_subscription_fanout_delivers_to_each_current_subscriber() {
        let bus = InMemoryBus::new();
        bus.subscribe("charlie", &["broadcast"]).await;
        bus.subscribe("dana", &["broadcast"]).await;

        bus.publish(AgentEnvelope::new("alice", "*", "broadcast", vec![1]))
            .await
            .unwrap();

        let charlie_msgs = bus.drain_for("charlie").await;
        assert_eq!(charlie_msgs.len(), 1);
        assert_eq!(charlie_msgs[0].payload, vec![1]);

        let dana_msgs = bus.drain_for("dana").await;
        assert_eq!(dana_msgs.len(), 1);
        assert_eq!(dana_msgs[0].payload, vec![1]);

        assert!(bus.is_empty().await);
    }

    #[tokio::test]
    async fn test_targeted_message_is_not_delivered_to_topic_subscriber() {
        let bus = InMemoryBus::new();
        bus.subscribe("charlie", &["private"]).await;

        bus.publish(AgentEnvelope::new("alice", "bob", "private", vec![]))
            .await
            .unwrap();

        assert!(bus.drain_for("charlie").await.is_empty());
        assert_eq!(bus.drain_for("bob").await.len(), 1);
    }

    #[tokio::test]
    async fn test_bus_full_error() {
        let bus = InMemoryBus::new();
        for _ in 0..10_002 {
            let env = AgentEnvelope::new("alice", "bob", "test", vec![]);
            if bus.publish(env).await.is_err() {
                return;
            }
        }
        panic!("should have returned BusFull");
    }

    #[tokio::test]
    async fn test_auto_id_generation() {
        let bus = InMemoryBus::new();
        let id = bus
            .publish(AgentEnvelope::new("alice", "bob", "test", vec![]))
            .await
            .unwrap();
        assert!(id.starts_with("msg_"));
    }
}

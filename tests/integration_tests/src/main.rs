fn main() {
    println!("agentOS integration tests — run with: cargo test -p agentos-integration-tests");
}

#[cfg(test)]
mod tests {
    use agentos_kernel::events::SystemEventType;
    use agentos_kernel::system::AgentOSSystem;
    use agentos_kernel::AgentSpec;

    #[tokio::test]
    async fn test_full_system_lifecycle() {
        let system = AgentOSSystem::new();
        let spec = AgentSpec::new("lifecycle-test", "Lifecycle Test Agent");
        let handle = system.spawn_agent(spec).await.unwrap();
        assert!(handle.is_running().await);

        let thought_id = system
            .record_thought("lifecycle-test", "first thought")
            .await;
        assert!(!thought_id.is_empty());

        let events = system.event_bus.read_all().await;
        assert!(events
            .iter()
            .any(|e| e.event_type == SystemEventType::AgentSpawned));
        assert!(events
            .iter()
            .any(|e| e.event_type == SystemEventType::ThoughtRecorded));

        system.shutdown_all().await;
        assert!(!handle.is_running().await);
    }

    #[tokio::test]
    async fn test_memory_and_vault_integration() {
        let system = AgentOSSystem::new();
        let spec = AgentSpec::new("mem-vault-test", "Memory Vault Test");
        let handle = system.spawn_agent(spec).await.unwrap();
        let agent_id = handle.id.clone();

        let mem_id = system
            .store_memory(&agent_id, "important data")
            .await
            .unwrap();
        assert!(!mem_id.is_empty());

        let results = system.search_memory(&agent_id, "data", 5).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "important data");

        system.set_secret(&agent_id, "API_KEY", "sk-test").await;
        let secret = system.get_secret(&agent_id, "API_KEY").await;
        assert_eq!(secret, Some("sk-test".into()));

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_event_bus_integration() {
        let system = AgentOSSystem::new();
        let spec = AgentSpec::new("event-test", "Event Test");
        let handle = system.spawn_agent(spec).await.unwrap();
        let agent_id = handle.id.clone();

        system
            .store_memory(&agent_id, "memory event")
            .await
            .unwrap();
        system.record_thought(&agent_id, "thought event").await;

        let events = system.event_bus.read_all().await;
        let memory_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == SystemEventType::MemoryStored)
            .collect();
        assert_eq!(memory_events.len(), 1);

        let agent_events = system.event_bus.read_for_agent(&agent_id).await;
        assert!(!agent_events.is_empty());

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_plugin_hooks() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let system = Arc::new(AgentOSSystem::new());
        let spawn_count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&spawn_count);

        system
            .agent_hooks
            .on_spawned
            .write()
            .await
            .push(Arc::new(move |_, _| {
                count_clone.fetch_add(1, Ordering::Relaxed);
            }));

        let spec = AgentSpec::new("hook-test", "Hook Test");
        system.spawn_agent(spec).await.unwrap();

        assert_eq!(spawn_count.load(Ordering::Relaxed), 1);
        system.shutdown_all().await;
    }
}

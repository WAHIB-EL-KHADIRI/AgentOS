use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use crate::system::AgentOSSystem;

pub type PluginHook = Arc<dyn Fn(&AgentOSSystem, &str) + Send + Sync>;
type ThoughtMemoryHook = Arc<dyn Fn(&AgentOSSystem, &str, &str) + Send + Sync>;

pub struct PluginRegistry {
    hooks: Arc<RwLock<HashMap<String, PluginHook>>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, name: &str, hook: PluginHook) {
        let mut hooks = self.hooks.write().await;
        hooks.insert(name.to_string(), hook);
        info!(plugin = %name, "plugin registered");
    }

    pub async fn unregister(&self, name: &str) {
        let mut hooks = self.hooks.write().await;
        hooks.remove(name);
        info!(plugin = %name, "plugin unregistered");
    }

    pub async fn list(&self) -> Vec<String> {
        let hooks = self.hooks.read().await;
        hooks.keys().cloned().collect()
    }

    pub async fn trigger(&self, system: &AgentOSSystem, agent_id: &str) {
        let hooks = self.hooks.read().await;
        for hook in hooks.values() {
            hook(system, agent_id);
        }
    }
}

/// Agent lifecycle hooks that plugins can register against.
/// These are deliberately separate from PluginRegistry to keep the API simple.
pub struct AgentHooks {
    pub on_spawned: Arc<RwLock<Vec<PluginHook>>>,
    pub on_stopped: Arc<RwLock<Vec<PluginHook>>>,
    pub on_thought: Arc<RwLock<Vec<ThoughtMemoryHook>>>,
    pub on_memory: Arc<RwLock<Vec<ThoughtMemoryHook>>>,
}

impl Default for AgentHooks {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentHooks {
    pub fn new() -> Self {
        Self {
            on_spawned: Arc::new(RwLock::new(Vec::new())),
            on_stopped: Arc::new(RwLock::new(Vec::new())),
            on_thought: Arc::new(RwLock::new(Vec::new())),
            on_memory: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn on_spawned(&self, system: &AgentOSSystem, agent_id: &str) {
        let hooks = self.on_spawned.read().await;
        for hook in hooks.iter() {
            hook(system, agent_id);
        }
    }

    pub async fn on_stopped(&self, system: &AgentOSSystem, agent_id: &str) {
        let hooks = self.on_stopped.read().await;
        for hook in hooks.iter() {
            hook(system, agent_id);
        }
    }

    pub async fn on_thought(&self, system: &AgentOSSystem, agent_id: &str, thought: &str) {
        let hooks = self.on_thought.read().await;
        for hook in hooks.iter() {
            hook(system, agent_id, thought);
        }
    }

    pub async fn on_memory(&self, system: &AgentOSSystem, agent_id: &str, content: &str) {
        let hooks = self.on_memory.read().await;
        for hook in hooks.iter() {
            hook(system, agent_id, content);
        }
    }
}

pub fn logging_hook() -> PluginHook {
    Arc::new(|system: &AgentOSSystem, agent_id: &str| {
        let _ = system;
        info!(agent = %agent_id, "agent lifecycle event");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_list() {
        let registry = PluginRegistry::new();
        registry.register("test", logging_hook()).await;
        let plugins = registry.list().await;
        assert_eq!(plugins, vec!["test"]);
    }

    #[tokio::test]
    async fn test_unregister() {
        let registry = PluginRegistry::new();
        registry.register("test", logging_hook()).await;
        registry.unregister("test").await;
        let plugins = registry.list().await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_agent_hooks_default() {
        let hooks = AgentHooks::new();
        assert!(hooks.on_spawned.read().await.is_empty());
    }
}

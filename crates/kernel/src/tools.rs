//! Runtime tool registry: tools an agent can invoke during its execution
//! loop. SDK tools (and any host-provided tools) are registered here per
//! agent, and `AgentOSSystem::run_agent_once` executes them when the LLM
//! response contains tool calls.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

/// A tool the runtime can execute on behalf of an agent.
///
/// `arguments` is the JSON value produced by the LLM tool call. The result
/// is either the tool output (fed back to the LLM and recorded in trace)
/// or an error string (also recorded, never fatal to the runtime).
#[async_trait]
pub trait RuntimeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    /// JSON Schema describing the tool arguments, if any.
    fn parameters(&self) -> Option<serde_json::Value> {
        None
    }

    async fn invoke(&self, arguments: &serde_json::Value) -> Result<String, String>;
}

impl fmt::Debug for dyn RuntimeTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeTool")
            .field("name", &self.name())
            .field("description", &self.description())
            .finish()
    }
}

/// Tools registered for a single agent, keyed by tool name.
type AgentTools = HashMap<String, Arc<dyn RuntimeTool>>;

/// Per-agent registry of runtime tools.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, AgentTools>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool for an agent. A tool with the same name replaces the
    /// previous registration.
    pub async fn register(&self, agent_id: &str, tool: Arc<dyn RuntimeTool>) {
        let mut tools = self.tools.write().await;
        tools
            .entry(agent_id.to_string())
            .or_default()
            .insert(tool.name().to_string(), tool);
    }

    pub async fn get(&self, agent_id: &str, name: &str) -> Option<Arc<dyn RuntimeTool>> {
        let tools = self.tools.read().await;
        tools.get(agent_id)?.get(name).cloned()
    }

    /// All tools registered for an agent, sorted by name for deterministic
    /// request payloads and trace output.
    pub async fn tools_for(&self, agent_id: &str) -> Vec<Arc<dyn RuntimeTool>> {
        let tools = self.tools.read().await;
        let mut list: Vec<_> = tools
            .get(agent_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        list.sort_by(|a, b| a.name().cmp(b.name()));
        list
    }

    pub async fn count_for(&self, agent_id: &str) -> usize {
        let tools = self.tools.read().await;
        tools.get(agent_id).map(|m| m.len()).unwrap_or(0)
    }

    /// Drop all tools registered for an agent (e.g. after shutdown).
    pub async fn remove_agent(&self, agent_id: &str) {
        let mut tools = self.tools.write().await;
        tools.remove(agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticTool {
        name: &'static str,
        reply: &'static str,
    }

    #[async_trait]
    impl RuntimeTool for StaticTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "static test tool"
        }

        async fn invoke(&self, _arguments: &serde_json::Value) -> Result<String, String> {
            Ok(self.reply.to_string())
        }
    }

    fn tool(name: &'static str, reply: &'static str) -> Arc<dyn RuntimeTool> {
        Arc::new(StaticTool { name, reply })
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let registry = ToolRegistry::new();
        registry.register("agent-1", tool("echo", "hi")).await;

        let found = registry.get("agent-1", "echo").await.unwrap();
        assert_eq!(found.name(), "echo");
        assert_eq!(found.invoke(&serde_json::json!({})).await.unwrap(), "hi");

        assert!(registry.get("agent-1", "missing").await.is_none());
        assert!(registry.get("agent-2", "echo").await.is_none());
    }

    #[tokio::test]
    async fn test_tools_for_sorted_and_isolated() {
        let registry = ToolRegistry::new();
        registry.register("agent-1", tool("zeta", "z")).await;
        registry.register("agent-1", tool("alpha", "a")).await;
        registry.register("agent-2", tool("other", "o")).await;

        let names: Vec<_> = registry
            .tools_for("agent-1")
            .await
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
        assert_eq!(registry.count_for("agent-2").await, 1);
        assert_eq!(registry.count_for("agent-3").await, 0);
    }

    #[tokio::test]
    async fn test_same_name_replaces() {
        let registry = ToolRegistry::new();
        registry.register("agent-1", tool("echo", "first")).await;
        registry.register("agent-1", tool("echo", "second")).await;

        assert_eq!(registry.count_for("agent-1").await, 1);
        let found = registry.get("agent-1", "echo").await.unwrap();
        assert_eq!(
            found.invoke(&serde_json::json!({})).await.unwrap(),
            "second"
        );
    }

    #[tokio::test]
    async fn test_remove_agent() {
        let registry = ToolRegistry::new();
        registry.register("agent-1", tool("echo", "hi")).await;
        registry.remove_agent("agent-1").await;
        assert_eq!(registry.count_for("agent-1").await, 0);
    }
}

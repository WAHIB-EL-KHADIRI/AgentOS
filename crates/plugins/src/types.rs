use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub hooks: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PluginDescriptor {
    pub path: String,
    pub manifest: PluginManifest,
    pub wasm_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginState {
    Loaded,
    Running,
    Error(String),
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginEvent {
    AgentSpawned {
        agent_id: String,
        plugin: String,
    },
    AgentStopped {
        agent_id: String,
        plugin: String,
    },
    Thought {
        agent_id: String,
        content: String,
        plugin: String,
    },
    ToolCall {
        agent_id: String,
        tool: String,
        input: String,
        plugin: String,
    },
}

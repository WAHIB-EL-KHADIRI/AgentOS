use serde::Deserialize;

use crate::agent::AgentSpec;
use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestRuntime {
    Native,
    Wasm,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PermissionEntry {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub access: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub runtime: Option<ManifestRuntime>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub restart: Option<RestartPolicy>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl AgentManifest {
    pub fn from_yaml(path: &str) -> AgentResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AgentError::ConfigError(format!("cannot read '{}': {}", path, e)))?;

        serde_yaml::from_str::<AgentManifest>(&content)
            .map_err(|e| AgentError::ConfigError(format!("invalid manifest in '{}': {}", path, e)))
    }

    pub fn into_spec(self, id: impl Into<String>) -> AgentSpec {
        let max_restarts = match &self.restart {
            Some(RestartPolicy::Always) | Some(RestartPolicy::OnFailure) => 10,
            Some(RestartPolicy::Never) => 0,
            None => 5,
        };

        AgentSpec {
            id: id.into(),
            name: self.name,
            prompt: self.prompt,
            capabilities: self.capabilities,
            max_restarts,
            heartbeat_timeout_secs: 30,
        }
    }
}

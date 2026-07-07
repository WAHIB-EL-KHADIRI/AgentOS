use serde::Deserialize;

use crate::agent::AgentSpec;
use crate::error::{AgentError, AgentResult};

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl AgentConfig {
    pub fn from_toml(path: &str) -> AgentResult<Self> {
        Self::validate_path(path)?;
        let content = std::fs::read_to_string(path)
            .map_err(|e| AgentError::ConfigError(format!("cannot read '{}': {}", path, e)))?;

        toml::from_str::<AgentConfig>(&content)
            .map_err(|e| AgentError::ConfigError(format!("invalid config in '{}': {}", path, e)))
    }

    pub fn from_yaml(path: &str) -> AgentResult<Self> {
        Self::validate_path(path)?;
        let manifest = crate::manifest::AgentManifest::from_yaml(path)?;
        Ok(Self {
            name: manifest.name,
            prompt: manifest.prompt,
            capabilities: manifest.capabilities,
        })
    }

    pub fn from_file(path: &str) -> AgentResult<Self> {
        Self::validate_path(path)?;
        let lower = path.to_lowercase();
        if lower.ends_with(".yaml") || lower.ends_with(".yml") {
            Self::from_yaml(path)
        } else if lower.ends_with(".toml") {
            Self::from_toml(path)
        } else {
            Err(AgentError::ConfigError(format!(
                "unsupported config format: '{}' (use .toml, .yaml, or .yml)",
                path
            )))
        }
    }

    fn validate_path(path: &str) -> AgentResult<()> {
        if path.contains("..") {
            return Err(AgentError::ConfigError(
                "path must not contain '..' traversal sequences".into(),
            ));
        }
        if path.contains('\0') {
            return Err(AgentError::ConfigError(
                "path must not contain null bytes".into(),
            ));
        }
        Ok(())
    }

    pub fn into_spec(self, id: impl Into<String>) -> AgentSpec {
        AgentSpec {
            id: id.into(),
            name: self.name,
            prompt: self.prompt,
            capabilities: self.capabilities,
            max_restarts: 5,
            heartbeat_timeout_secs: 30,
        }
    }
}

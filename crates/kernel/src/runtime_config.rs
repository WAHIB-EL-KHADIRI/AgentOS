use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_max_agents")]
    pub max_agents: usize,

    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u64,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default = "default_data_dir")]
    pub data_dir: String,

    #[serde(default)]
    pub agents: Vec<AgentConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfigEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
}

fn default_http_port() -> u16 {
    8080
}
fn default_grpc_port() -> u16 {
    50051
}
fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_max_agents() -> usize {
    100
}
fn default_heartbeat_timeout() -> u64 {
    30
}
fn default_log_level() -> String {
    "info".into()
}
fn default_data_dir() -> String {
    "data".into()
}
fn default_max_restarts() -> u32 {
    5
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            http_port: default_http_port(),
            grpc_port: default_grpc_port(),
            host: default_host(),
            max_agents: default_max_agents(),
            heartbeat_timeout_secs: default_heartbeat_timeout(),
            log_level: default_log_level(),
            data_dir: default_data_dir(),
            agents: Vec::new(),
        }
    }
}

impl RuntimeConfig {
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;
        let mut config: RuntimeConfig = toml::from_str(&content)?;
        config.apply_env_overrides();
        Ok(config)
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("AGENTOS_HTTP_PORT") {
            if let Ok(port) = v.parse() {
                self.http_port = port;
            }
        }
        if let Ok(v) = std::env::var("AGENTOS_GRPC_PORT") {
            if let Ok(port) = v.parse() {
                self.grpc_port = port;
            }
        }
        if let Ok(v) = std::env::var("AGENTOS_HOST") {
            self.host = v;
        }
        if let Ok(v) = std::env::var("AGENTOS_MAX_AGENTS") {
            if let Ok(n) = v.parse() {
                self.max_agents = n;
            }
        }
        if let Ok(v) = std::env::var("AGENTOS_LOG_LEVEL") {
            self.log_level = v;
        }
        if let Ok(v) = std::env::var("AGENTOS_DATA_DIR") {
            self.data_dir = v;
        }
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.host, self.http_port)
    }

    pub fn grpc_addr(&self) -> String {
        format!("{}:{}", self.host, self.grpc_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = RuntimeConfig::default();
        assert_eq!(cfg.http_port, 8080);
        assert_eq!(cfg.grpc_port, 50051);
        assert_eq!(cfg.host, "127.0.0.1");
    }

    #[test]
    fn test_config_env_override() {
        let orig_port = std::env::var("AGENTOS_HTTP_PORT").ok();
        let orig_host = std::env::var("AGENTOS_HOST").ok();
        std::env::set_var("AGENTOS_HTTP_PORT", "9090");
        std::env::set_var("AGENTOS_HOST", "0.0.0.0");
        let mut cfg = RuntimeConfig::default();
        cfg.apply_env_overrides();
        assert_eq!(cfg.http_port, 9090);
        assert_eq!(cfg.host, "0.0.0.0");
        if let Some(val) = orig_port {
            std::env::set_var("AGENTOS_HTTP_PORT", val);
        } else {
            std::env::remove_var("AGENTOS_HTTP_PORT");
        }
        if let Some(val) = orig_host {
            std::env::set_var("AGENTOS_HOST", val);
        } else {
            std::env::remove_var("AGENTOS_HOST");
        }
    }

    #[test]
    fn test_listen_addr() {
        let cfg = RuntimeConfig::default();
        assert_eq!(cfg.listen_addr(), "127.0.0.1:8080");
        assert_eq!(cfg.grpc_addr(), "127.0.0.1:50051");
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
http_port = 3000
host = "0.0.0.0"
max_agents = 50

[[agents]]
id = "agent-1"
name = "Test Agent"
prompt = "You are a test agent"
capabilities = ["search", "memory"]
"#;
        let config: RuntimeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.http_port, 3000);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.max_agents, 50);
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].id, "agent-1");
        assert_eq!(config.agents[0].capabilities, vec!["search", "memory"]);
    }
}

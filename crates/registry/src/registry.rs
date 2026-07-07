use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type RegistryResult<T> = Result<T, RegistryError>;

const MAX_ENDPOINT_LEN: usize = 1024;
const MAX_NAME_LEN: usize = 256;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("service '{0}' already registered")]
    AlreadyRegistered(String),

    #[error("service '{0}' not found")]
    NotFound(String),

    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceHealth {
    Unknown,
    Healthy,
    Degraded(String),
    Unreachable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    pub name: String,
    pub endpoint: String,
    pub capabilities: Vec<String>,
    pub health: ServiceHealth,
    pub registered_at_ms: u64,
    pub metadata: HashMap<String, String>,
}

impl ServiceDescriptor {
    pub fn new(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        let mut endpoint: String = endpoint.into();
        let mut name: String = name.into();
        if endpoint.len() > MAX_ENDPOINT_LEN {
            endpoint.truncate(MAX_ENDPOINT_LEN);
        }
        if name.len() > MAX_NAME_LEN {
            name.truncate(MAX_NAME_LEN);
        }
        Self {
            name,
            endpoint,
            capabilities: Vec::new(),
            health: ServiceHealth::Unknown,
            registered_at_ms: chrono::Utc::now().timestamp_millis() as u64,
            metadata: HashMap::new(),
        }
    }

    pub fn validate_endpoint(endpoint: &str) -> Result<(), String> {
        if endpoint.is_empty() {
            return Err("endpoint must not be empty".into());
        }
        if endpoint.contains('\0') {
            return Err("endpoint must not contain null bytes".into());
        }
        if endpoint.len() > MAX_ENDPOINT_LEN {
            return Err(format!("endpoint too long (max {MAX_ENDPOINT_LEN} bytes)"));
        }
        Ok(())
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    services: HashMap<String, ServiceDescriptor>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: ServiceDescriptor) -> RegistryResult<()> {
        if self.services.contains_key(&descriptor.name) {
            return Err(RegistryError::AlreadyRegistered(descriptor.name));
        }
        ServiceDescriptor::validate_endpoint(&descriptor.endpoint)
            .map_err(RegistryError::InvalidEndpoint)?;
        self.services.insert(descriptor.name.clone(), descriptor);
        Ok(())
    }

    pub fn register_or_update(&mut self, descriptor: ServiceDescriptor) {
        if ServiceDescriptor::validate_endpoint(&descriptor.endpoint).is_err() {
            return;
        }
        self.services.insert(descriptor.name.clone(), descriptor);
    }

    pub fn get(&self, name: &str) -> Option<&ServiceDescriptor> {
        self.services.get(name)
    }

    pub fn unregister(&mut self, name: &str) -> RegistryResult<()> {
        self.services
            .remove(name)
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))?;
        Ok(())
    }

    pub fn discover_by_capability(&self, capability: &str) -> Vec<&ServiceDescriptor> {
        self.services
            .values()
            .filter(|service| service.capabilities.iter().any(|item| item == capability))
            .collect()
    }

    pub fn list(&self) -> Vec<&ServiceDescriptor> {
        self.services.values().collect()
    }

    pub fn update_health(&mut self, name: &str, health: ServiceHealth) -> RegistryResult<()> {
        let service = self
            .services
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))?;
        service.health = health;
        Ok(())
    }

    pub fn healthy_services(&self) -> Vec<&ServiceDescriptor> {
        self.services
            .values()
            .filter(|s| s.health == ServiceHealth::Healthy)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.services.len()
    }

    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let mut registry = Registry::new();
        let desc = ServiceDescriptor::new("agent-1", "localhost:9001")
            .with_capabilities(vec!["memory".into()]);
        registry.register(desc).unwrap();
        assert!(registry.get("agent-1").is_some());
    }

    #[test]
    fn test_duplicate_registration_fails() {
        let mut registry = Registry::new();
        let desc = ServiceDescriptor::new("agent-1", "localhost:9001");
        registry.register(desc).unwrap();
        let desc2 = ServiceDescriptor::new("agent-1", "localhost:9002");
        assert!(registry.register(desc2).is_err());
    }

    #[test]
    fn test_register_or_update() {
        let mut registry = Registry::new();
        registry.register_or_update(ServiceDescriptor::new("agent-1", "localhost:9001"));
        registry.register_or_update(ServiceDescriptor::new("agent-1", "localhost:9002"));
        assert_eq!(registry.get("agent-1").unwrap().endpoint, "localhost:9002");
    }

    #[test]
    fn test_discover_by_capability() {
        let mut registry = Registry::new();
        registry
            .register(
                ServiceDescriptor::new("agent-1", "addr1")
                    .with_capabilities(vec!["memory".into(), "search".into()]),
            )
            .unwrap();
        registry
            .register(
                ServiceDescriptor::new("agent-2", "addr2").with_capabilities(vec!["search".into()]),
            )
            .unwrap();
        registry
            .register(
                ServiceDescriptor::new("agent-3", "addr3")
                    .with_capabilities(vec!["compute".into()]),
            )
            .unwrap();

        let search_services = registry.discover_by_capability("search");
        assert_eq!(search_services.len(), 2);

        let compute_services = registry.discover_by_capability("compute");
        assert_eq!(compute_services.len(), 1);
    }

    #[test]
    fn test_unregister() {
        let mut registry = Registry::new();
        registry
            .register(ServiceDescriptor::new("agent-1", "addr"))
            .unwrap();
        registry.unregister("agent-1").unwrap();
        assert!(registry.get("agent-1").is_none());
    }

    #[test]
    fn test_health_tracking() {
        let mut registry = Registry::new();
        registry
            .register(ServiceDescriptor::new("agent-1", "addr"))
            .unwrap();
        registry
            .update_health("agent-1", ServiceHealth::Healthy)
            .unwrap();
        assert_eq!(registry.healthy_services().len(), 1);
        registry
            .update_health("agent-1", ServiceHealth::Unreachable("timeout".into()))
            .unwrap();
        assert_eq!(registry.healthy_services().len(), 0);
    }

    #[test]
    fn test_list_services() {
        let mut registry = Registry::new();
        registry
            .register(ServiceDescriptor::new("a", "addr1"))
            .unwrap();
        registry
            .register(ServiceDescriptor::new("b", "addr2"))
            .unwrap();
        assert_eq!(registry.list().len(), 2);
    }
}

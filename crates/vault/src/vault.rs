use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_AUDIT_LOG_ENTRIES: usize = 10_000;

pub type VaultResult<T> = Result<T, VaultError>;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("secret not found: agent '{0}', key '{1}'")]
    SecretNotFound(String, String),

    #[error("secret not found in scope '{0}', key '{1}'")]
    ScopeSecretNotFound(String, String),

    #[error("access denied: agent '{0}' does not have permission '{1}'")]
    AccessDenied(String, String),

    #[error("scope '{0}' not found")]
    ScopeNotFound(String),

    #[error("vault encryption error: {0}")]
    Encryption(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretValue {
    inner: String,
    #[serde(skip)]
    access_count: u64,
}

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: value.into(),
            access_count: 0,
        }
    }

    pub fn expose_for_runtime(&mut self) -> &str {
        self.access_count += 1;
        &self.inner
    }

    pub fn expose(&self) -> &str {
        &self.inner
    }

    pub fn access_count(&self) -> u64 {
        self.access_count
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.inner.as_bytes());
        hex::encode(hasher.finalize())
    }
}

impl std::fmt::Display for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretValue[***]")
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Vault {
    secrets: HashMap<String, HashMap<String, SecretValue>>,
    /// Shared scopes accessible by multiple agents
    scopes: HashMap<String, HashMap<String, SecretValue>>,
    /// Maps each agent to the set of scopes they can access
    agent_scopes: HashMap<String, HashSet<String>>,
    #[serde(skip)]
    audit_log: VecDeque<VaultAuditEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultAuditEntry {
    pub agent_id: String,
    pub key: String,
    pub timestamp_ms: u64,
    pub action: String,
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, agent_id: &str, key: impl Into<String>, value: impl Into<String>) {
        let key: String = key.into();
        if key.is_empty() {
            return;
        }
        self.secrets
            .entry(agent_id.to_string())
            .or_default()
            .insert(key, SecretValue::new(value));
    }

    pub fn get(&mut self, agent_id: &str, key: &str) -> VaultResult<&mut SecretValue> {
        self.push_audit_entry(VaultAuditEntry {
            agent_id: agent_id.to_string(),
            key: key.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            action: "read".into(),
        });

        let secrets = self
            .secrets
            .get_mut(agent_id)
            .ok_or_else(|| VaultError::SecretNotFound(agent_id.to_string(), key.to_string()))?;

        let secret = secrets
            .get_mut(key)
            .ok_or_else(|| VaultError::SecretNotFound(agent_id.to_string(), key.to_string()))?;

        Ok(secret)
    }

    pub fn remove(&mut self, agent_id: &str, key: &str) -> VaultResult<()> {
        let secrets = self
            .secrets
            .get_mut(agent_id)
            .ok_or_else(|| VaultError::SecretNotFound(agent_id.to_string(), key.to_string()))?;

        secrets
            .remove(key)
            .ok_or_else(|| VaultError::SecretNotFound(agent_id.to_string(), key.to_string()))?;

        Ok(())
    }

    pub fn list_keys(&self, agent_id: &str) -> Vec<String> {
        self.secrets
            .get(agent_id)
            .map(|secrets| secrets.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Agent ids that have at least one stored secret.
    pub fn agent_ids(&self) -> Vec<String> {
        self.secrets.keys().cloned().collect()
    }

    /// Read a secret value without recording an audit access. Intended for
    /// system-level persistence and migration paths, not for agent reads.
    pub fn peek(&self, agent_id: &str, key: &str) -> Option<&str> {
        self.secrets
            .get(agent_id)?
            .get(key)
            .map(|secret| secret.expose())
    }

    pub fn has_secret(&self, agent_id: &str, key: &str) -> bool {
        self.secrets
            .get(agent_id)
            .and_then(|s| s.get(key))
            .is_some()
    }

    pub fn audit_log(&self) -> &VecDeque<VaultAuditEntry> {
        &self.audit_log
    }

    fn push_audit_entry(&mut self, entry: VaultAuditEntry) {
        if self.audit_log.len() >= MAX_AUDIT_LOG_ENTRIES {
            self.audit_log.pop_front();
        }
        self.audit_log.push_back(entry);
    }

    // -----------------------------------------------------------------------
    // Secret Scopes
    // -----------------------------------------------------------------------

    pub fn create_scope(&mut self, scope: &str) {
        self.scopes.entry(scope.to_string()).or_default();
    }

    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains_key(scope)
    }

    pub fn list_scopes(&self) -> Vec<String> {
        self.scopes.keys().cloned().collect()
    }

    pub fn put_in_scope(
        &mut self,
        scope: &str,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> VaultResult<()> {
        let scope_secrets = self
            .scopes
            .get_mut(scope)
            .ok_or_else(|| VaultError::ScopeNotFound(scope.to_string()))?;
        scope_secrets.insert(key.into(), SecretValue::new(value));
        Ok(())
    }

    pub fn get_from_scope(&mut self, scope: &str, key: &str) -> VaultResult<&mut SecretValue> {
        self.push_audit_entry(VaultAuditEntry {
            agent_id: format!("scope:{scope}"),
            key: key.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            action: "scope_read".into(),
        });

        let scope_secrets = self
            .scopes
            .get_mut(scope)
            .ok_or_else(|| VaultError::ScopeNotFound(scope.to_string()))?;
        let secret = scope_secrets
            .get_mut(key)
            .ok_or_else(|| VaultError::ScopeSecretNotFound(scope.to_string(), key.to_string()))?;

        Ok(secret)
    }

    pub fn remove_from_scope(&mut self, scope: &str, key: &str) -> VaultResult<()> {
        let scope_secrets = self
            .scopes
            .get_mut(scope)
            .ok_or_else(|| VaultError::ScopeNotFound(scope.to_string()))?;
        scope_secrets
            .remove(key)
            .ok_or_else(|| VaultError::ScopeSecretNotFound(scope.to_string(), key.to_string()))?;
        Ok(())
    }

    pub fn list_scope_keys(&self, scope: &str) -> VaultResult<Vec<String>> {
        let scope_secrets = self
            .scopes
            .get(scope)
            .ok_or_else(|| VaultError::ScopeNotFound(scope.to_string()))?;
        Ok(scope_secrets.keys().cloned().collect())
    }

    pub fn assign_agent_to_scope(&mut self, agent_id: &str, scope: &str) {
        self.agent_scopes
            .entry(agent_id.to_string())
            .or_default()
            .insert(scope.to_string());
    }

    pub fn unassign_agent_from_scope(&mut self, agent_id: &str, scope: &str) {
        if let Some(scopes) = self.agent_scopes.get_mut(agent_id) {
            scopes.remove(scope);
        }
    }

    pub fn agent_scopes(&self, agent_id: &str) -> Vec<String> {
        self.agent_scopes
            .get(agent_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Try to get a secret from the agent's own store first, then fall back
    /// to each scope in order.
    pub fn get_with_scope_fallback(
        &mut self,
        agent_id: &str,
        key: &str,
        scopes: &[&str],
    ) -> VaultResult<&mut SecretValue> {
        // Try agent-local secret first
        if self.has_secret(agent_id, key) {
            return self.get(agent_id, key);
        }

        // Try each scope
        for &scope in scopes {
            if self.get_from_scope(scope, key).is_ok() {
                return self.get_from_scope(scope, key);
            }
        }

        Err(VaultError::SecretNotFound(
            agent_id.to_string(),
            key.to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_put_and_get() {
        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123456");
        let secret = vault.get("agent-1", "API_KEY").unwrap();
        assert_eq!(secret.expose(), "sk-123456");
    }

    #[test]
    fn test_vault_missing_secret() {
        let mut vault = Vault::new();
        let result = vault.get("agent-1", "MISSING");
        assert!(result.is_err());
    }

    #[test]
    fn test_vault_remove() {
        let mut vault = Vault::new();
        vault.put("agent-1", "KEY", "value");
        vault.remove("agent-1", "KEY").unwrap();
        assert!(!vault.has_secret("agent-1", "KEY"));
    }

    #[test]
    fn test_vault_list_keys() {
        let mut vault = Vault::new();
        vault.put("agent-1", "KEY_1", "v1");
        vault.put("agent-1", "KEY_2", "v2");
        let keys = vault.list_keys("agent-1");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"KEY_1".to_string()));
        assert!(keys.contains(&"KEY_2".to_string()));
    }

    #[test]
    fn test_secret_value_hash() {
        let secret = SecretValue::new("my-secret");
        let hash = secret.hash();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_secret_value_display_hides_value() {
        let secret = SecretValue::new("secret123");
        let display = format!("{secret}");
        assert_eq!(display, "SecretValue[***]");
        assert!(!display.contains("secret123"));
    }

    #[test]
    fn test_vault_audit_log() {
        let mut vault = Vault::new();
        vault.put("agent-1", "KEY", "value");
        let _ = vault.get("agent-1", "KEY").unwrap();
        assert_eq!(vault.audit_log().len(), 1);
        assert_eq!(vault.audit_log()[0].action, "read");
        assert_eq!(vault.audit_log()[0].key, "KEY");
    }

    #[test]
    fn test_access_count_tracking() {
        let mut vault = Vault::new();
        vault.put("agent-1", "KEY", "value");
        let secret = vault.get("agent-1", "KEY").unwrap();
        secret.expose_for_runtime();
        assert_eq!(secret.access_count(), 1);
        secret.expose_for_runtime();
        assert_eq!(secret.access_count(), 2);
    }

    // -----------------------------------------------------------------------
    // Secret Scopes tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_scope() {
        let mut vault = Vault::new();
        vault.create_scope("production");
        assert!(vault.has_scope("production"));
        assert!(!vault.has_scope("nonexistent"));
    }

    #[test]
    fn test_put_and_get_from_scope() {
        let mut vault = Vault::new();
        vault.create_scope("database");
        vault.put_in_scope("database", "DB_PASS", "s3cr3t").unwrap();
        let secret = vault.get_from_scope("database", "DB_PASS").unwrap();
        assert_eq!(secret.expose(), "s3cr3t");
    }

    #[test]
    fn test_get_from_nonexistent_scope() {
        let mut vault = Vault::new();
        let result = vault.get_from_scope("missing", "key");
        assert!(result.is_err());
        assert!(matches!(result, Err(VaultError::ScopeNotFound(_))));
    }

    #[test]
    fn test_list_scopes() {
        let mut vault = Vault::new();
        vault.create_scope("prod");
        vault.create_scope("staging");
        let scopes = vault.list_scopes();
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&"prod".to_string()));
        assert!(scopes.contains(&"staging".to_string()));
    }

    #[test]
    fn test_remove_from_scope() {
        let mut vault = Vault::new();
        vault.create_scope("cache");
        vault
            .put_in_scope("cache", "REDIS_URL", "redis://...")
            .unwrap();
        vault.remove_from_scope("cache", "REDIS_URL").unwrap();
        assert!(vault.list_scope_keys("cache").unwrap().is_empty());
    }

    #[test]
    fn test_agent_scope_assignment() {
        let mut vault = Vault::new();
        vault.create_scope("production");
        vault.assign_agent_to_scope("agent-1", "production");
        let scopes = vault.agent_scopes("agent-1");
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0], "production");
    }

    #[test]
    fn test_agent_unassign_from_scope() {
        let mut vault = Vault::new();
        vault.create_scope("production");
        vault.assign_agent_to_scope("agent-1", "production");
        vault.unassign_agent_from_scope("agent-1", "production");
        assert!(vault.agent_scopes("agent-1").is_empty());
    }

    #[test]
    fn test_get_with_scope_fallback_agent_first() {
        let mut vault = Vault::new();
        vault.create_scope("shared");
        vault.put("agent-1", "API_KEY", "agent-secret");
        vault
            .put_in_scope("shared", "API_KEY", "scope-secret")
            .unwrap();

        // Agent-local takes priority over scope
        let secret = vault
            .get_with_scope_fallback("agent-1", "API_KEY", &["shared"])
            .unwrap();
        assert_eq!(secret.expose(), "agent-secret");
    }

    #[test]
    fn test_get_with_scope_fallback_scope() {
        let mut vault = Vault::new();
        vault.create_scope("shared");
        vault
            .put_in_scope("shared", "DB_PASS", "scope-db-pass")
            .unwrap();

        // Falls back to scope when agent doesn't have the secret
        let secret = vault
            .get_with_scope_fallback("agent-1", "DB_PASS", &["shared"])
            .unwrap();
        assert_eq!(secret.expose(), "scope-db-pass");
    }

    #[test]
    fn test_get_with_scope_fallback_all_missing() {
        let mut vault = Vault::new();
        vault.create_scope("shared");
        let result = vault.get_with_scope_fallback("agent-1", "MISSING", &["shared"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_audit_log() {
        let mut vault = Vault::new();
        vault.create_scope("audited");
        vault.put_in_scope("audited", "KEY", "val").unwrap();
        let _ = vault.get_from_scope("audited", "KEY").unwrap();
        let log = vault.audit_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].action, "scope_read");
        assert!(log[0].agent_id.contains("audited"));
    }
}

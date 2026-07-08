use std::path::{Path, PathBuf};

use agentos_trace::RecordedThought;
use agentos_vault::{Vault, VaultEncryption};
use serde::{Deserialize, Serialize};

use crate::journal::RecordedSession;
use tokio::io::AsyncWriteExt;

use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSnapshot {
    pub agent_id: String,
    pub thoughts: Vec<RecordedThought>,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSnapshot {
    pub secrets: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    pub captured_at_ms: u64,
}

#[derive(Debug)]
pub struct Persistence {
    data_dir: PathBuf,
}

impl Persistence {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub async fn ensure_dirs(&self) -> AgentResult<()> {
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create data dir: {e}")))?;
        tokio::fs::create_dir_all(self.data_dir.join("traces"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create traces dir: {e}")))?;
        tokio::fs::create_dir_all(self.data_dir.join("vault"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create vault dir: {e}")))?;
        tokio::fs::create_dir_all(self.data_dir.join("journals"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot create journals dir: {e}")))?;
        Ok(())
    }

    pub async fn save_trace(
        &self,
        agent_id: &str,
        thoughts: &[RecordedThought],
    ) -> AgentResult<()> {
        let snapshot = TraceSnapshot {
            agent_id: agent_id.to_string(),
            thoughts: thoughts.to_vec(),
            captured_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        };

        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| AgentError::Internal(format!("serialization error: {e}")))?;

        let path = self
            .data_dir
            .join("traces")
            .join(format!("{agent_id}.json"));

        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write trace file: {e}")))?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write trace: {e}")))?;

        Ok(())
    }

    pub async fn load_trace(&self, agent_id: &str) -> AgentResult<Vec<RecordedThought>> {
        let path = self
            .data_dir
            .join("traces")
            .join(format!("{agent_id}.json"));

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read trace file: {e}")))?;

        let snapshot: TraceSnapshot = serde_json::from_str(&content)
            .map_err(|e| AgentError::Internal(format!("cannot parse trace: {e}")))?;

        Ok(snapshot.thoughts)
    }

    fn vault_path(&self) -> PathBuf {
        self.data_dir.join("vault").join("secrets.enc")
    }

    /// Persist the vault encrypted with AES-256-GCM. Secrets are never
    /// written to disk in plaintext: encryption is a required argument,
    /// not an option.
    pub async fn save_vault(&self, vault: &Vault, encryption: &VaultEncryption) -> AgentResult<()> {
        let ciphertext = encryption
            .encrypt_json(vault)
            .map_err(|e| AgentError::Internal(format!("vault encryption error: {e}")))?;

        let path = self.vault_path();
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write vault file: {e}")))?;

        file.write_all(&ciphertext)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write vault: {e}")))?;

        Ok(())
    }

    /// Load and decrypt the persisted vault. Returns an empty vault when no
    /// file exists yet; fails when the key does not match the file.
    pub async fn load_vault(&self, encryption: &VaultEncryption) -> AgentResult<Vault> {
        let path = self.vault_path();

        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Vault::new());
        }

        let ciphertext = tokio::fs::read(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read vault file: {e}")))?;

        let vault: Vault = encryption.decrypt_json(&ciphertext).map_err(|e| {
            AgentError::Internal(format!(
                "cannot decrypt vault (wrong AGENTOS_VAULT_KEY?): {e}"
            ))
        })?;

        Ok(vault)
    }

    fn journal_path(&self, agent_id: &str) -> PathBuf {
        self.data_dir
            .join("journals")
            .join(format!("{}.json", sanitize_file_id(agent_id)))
    }

    /// Persist a recorded execution session (LLM exchanges + tool results).
    pub async fn save_journal(&self, session: &RecordedSession) -> AgentResult<()> {
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| AgentError::Internal(format!("journal serialization error: {e}")))?;

        let path = self.journal_path(&session.agent_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AgentError::Internal(format!("cannot create journals dir: {e}")))?;
        }

        tokio::fs::write(&path, json.as_bytes())
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write journal: {e}")))?;
        Ok(())
    }

    /// Load the recorded session for an agent, if one was journaled.
    pub async fn load_journal(&self, agent_id: &str) -> AgentResult<RecordedSession> {
        let path = self.journal_path(agent_id);
        let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
            AgentError::Internal(format!(
                "no recorded session for '{agent_id}' at {}: {e}",
                path.display()
            ))
        })?;
        serde_json::from_str(&content)
            .map_err(|e| AgentError::Internal(format!("cannot parse journal: {e}")))
    }

    pub async fn list_journals(&self) -> AgentResult<Vec<String>> {
        let dir = self.data_dir.join("journals");
        if !tokio::fs::try_exists(&dir).await.unwrap_or(false) {
            return Ok(Vec::new());
        }
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read journals dir: {e}")))?;

        let mut journals = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read entry: {e}")))?
        {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".json") {
                    journals.push(stem.to_string());
                }
            }
        }
        journals.sort();
        Ok(journals)
    }

    pub async fn list_traces(&self) -> AgentResult<Vec<String>> {
        let mut dir = tokio::fs::read_dir(self.data_dir.join("traces"))
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read traces dir: {e}")))?;

        let mut traces = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read entry: {e}")))?
        {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    traces.push(name.trim_end_matches(".json").to_string());
                }
            }
        }

        Ok(traces)
    }
}

/// Keep journal filenames safe regardless of agent id contents.
fn sanitize_file_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentos_trace::TraceRecorder;

    #[tokio::test]
    async fn test_persistence_save_and_load_trace() {
        let dir = std::env::temp_dir().join(format!("agentos_test_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let mut recorder = TraceRecorder::new();
        recorder.record_checkpoint("test-agent", "step 1");
        recorder.record_checkpoint("test-agent", "step 2");

        persist
            .save_trace("test-agent", recorder.thoughts())
            .await
            .unwrap();

        let loaded = persist.load_trace("test-agent").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content, "step 1");
        assert_eq!(loaded[1].content, "step 2");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_persistence_save_and_load_vault_encrypted() {
        let dir = std::env::temp_dir().join(format!("agentos_test_vault_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();
        let encryption = VaultEncryption::new();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123");
        vault.put("agent-1", "SECRET", "value");

        persist.save_vault(&vault, &encryption).await.unwrap();

        let loaded = persist.load_vault(&encryption).await.unwrap();
        assert!(loaded.has_secret("agent-1", "API_KEY"));
        assert!(loaded.has_secret("agent-1", "SECRET"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_vault_file_never_contains_plaintext_secrets() {
        let dir =
            std::env::temp_dir().join(format!("agentos_test_vault_plain_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();
        let encryption = VaultEncryption::new();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-super-secret-value");
        persist.save_vault(&vault, &encryption).await.unwrap();

        let raw = std::fs::read(dir.join("vault").join("secrets.enc")).unwrap();
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(!raw_text.contains("sk-super-secret-value"));
        assert!(!raw_text.contains("API_KEY"));
        assert!(serde_json::from_slice::<serde_json::Value>(&raw).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_vault_load_with_wrong_key_fails() {
        let dir = std::env::temp_dir().join(format!(
            "agentos_test_vault_wrongkey_{}",
            uuid::Uuid::new_v4()
        ));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123");
        persist
            .save_vault(&vault, &VaultEncryption::new())
            .await
            .unwrap();

        let result = persist.load_vault(&VaultEncryption::new()).await;
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_vault_load_missing_file_returns_empty() {
        let dir = std::env::temp_dir().join(format!(
            "agentos_test_vault_missing_{}",
            uuid::Uuid::new_v4()
        ));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let loaded = persist.load_vault(&VaultEncryption::new()).await.unwrap();
        assert!(loaded.list_keys("any-agent").is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_list_traces() {
        let dir = std::env::temp_dir().join(format!("agentos_test_list_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let recorder = TraceRecorder::new();
        persist
            .save_trace("agent-a", recorder.thoughts())
            .await
            .unwrap();
        persist
            .save_trace("agent-b", recorder.thoughts())
            .await
            .unwrap();

        let traces = persist.list_traces().await.unwrap();
        assert!(traces.contains(&"agent-a".to_string()));
        assert!(traces.contains(&"agent-b".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

use std::path::{Path, PathBuf};

use agentos_trace::RecordedThought;
use agentos_vault::Vault;
use serde::{Deserialize, Serialize};
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

    pub async fn save_vault(&self, vault: &Vault) -> AgentResult<()> {
        let json = serde_json::to_string_pretty(vault)
            .map_err(|e| AgentError::Internal(format!("serialization error: {e}")))?;

        let path = self.data_dir.join("vault").join("secrets.json");

        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write vault file: {e}")))?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| AgentError::Internal(format!("cannot write vault: {e}")))?;

        Ok(())
    }

    pub async fn load_vault(&self) -> AgentResult<Vault> {
        let path = self.data_dir.join("vault").join("secrets.json");

        if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Vault::new());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| AgentError::Internal(format!("cannot read vault file: {e}")))?;

        let vault: Vault = serde_json::from_str(&content)
            .map_err(|e| AgentError::Internal(format!("cannot parse vault: {e}")))?;

        Ok(vault)
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
    async fn test_persistence_save_and_load_vault() {
        let dir = std::env::temp_dir().join(format!("agentos_test_vault_{}", uuid::Uuid::new_v4()));
        let persist = Persistence::new(&dir);
        persist.ensure_dirs().await.unwrap();

        let mut vault = Vault::new();
        vault.put("agent-1", "API_KEY", "sk-123");
        vault.put("agent-1", "SECRET", "value");

        persist.save_vault(&vault).await.unwrap();

        let loaded = persist.load_vault().await.unwrap();
        assert!(loaded.has_secret("agent-1", "API_KEY"));
        assert!(loaded.has_secret("agent-1", "SECRET"));

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

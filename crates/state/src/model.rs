use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The portable JSON state model version written by this release.
pub const CURRENT_STATE_VERSION: u32 = 1;

/// Version assigned to portable files written before a version field existed.
pub const LEGACY_STATE_VERSION: u32 = 1;

const _: () = assert!(LEGACY_STATE_VERSION == 1);

/// A log record stored with an agent's operational state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredLogEntry {
    pub agent_id: String,
    pub event_type: String,
    pub message: String,
    pub timestamp_ms: u64,
}

/// The latest stored operational record for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAgentEntry {
    pub agent_id: String,
    pub name: String,
    pub config_path: String,
    pub state: String,
    #[serde(default)]
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

/// A trace checkpoint stored with an agent's operational state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredThoughtEntry {
    pub checkpoint_id: String,
    pub agent_id: String,
    pub content: String,
    pub timestamp_ms: u64,
    pub parent_checkpoint_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// A complete operational state snapshot passed to and from a [`StateBackend`](crate::StateBackend).
///
/// The public fields let third-party backends map the same state into their own
/// storage schema. `version` is the portable state format version; an SQLite
/// backend's schema version is a separate concern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    #[serde(default = "legacy_state_version")]
    pub version: u32,
    pub agents: Vec<StoredAgentEntry>,
    pub logs: Vec<StoredLogEntry>,
    pub thoughts: Vec<StoredThoughtEntry>,
}

fn legacy_state_version() -> u32 {
    LEGACY_STATE_VERSION
}

impl Default for StateSnapshot {
    fn default() -> Self {
        Self {
            version: CURRENT_STATE_VERSION,
            agents: Vec::new(),
            logs: Vec::new(),
            thoughts: Vec::new(),
        }
    }
}

impl StateSnapshot {
    /// Reject a portable snapshot written by a newer AgentOS state format.
    pub fn validate_version(&self) -> crate::StateResult<()> {
        if self.version > CURRENT_STATE_VERSION {
            return Err(crate::StateError::UnsupportedVersion {
                found: self.version,
                supported: CURRENT_STATE_VERSION,
            });
        }
        Ok(())
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use crate::{StateBackend, StateBackendKind, StateError, StateResult, StateSnapshot};

/// File-backed JSON storage for portable state snapshots.
pub struct JsonStateBackend {
    path: PathBuf,
}

impl JsonStateBackend {
    /// Create a backend for the given state file.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Return the configured state file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl StateBackend for JsonStateBackend {
    fn kind(&self) -> StateBackendKind {
        StateBackendKind::JSON
    }

    fn load_state(&self) -> StateResult<StateSnapshot> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StateSnapshot::default());
            }
            Err(error) => return Err(StateError::io(self.path.clone(), error)),
        };

        if content.trim().is_empty() {
            return Ok(StateSnapshot::default());
        }

        serde_json::from_str(&content).map_err(StateError::invalid_data)
    }

    fn save_state(&self, state: &StateSnapshot) -> StateResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| StateError::io(parent, error))?;
        }

        let content = serde_json::to_string_pretty(state)
            .map_err(|error| StateError::backend("could not encode JSON state", error))?;
        fs::write(&self.path, content).map_err(|error| StateError::io(self.path.clone(), error))
    }
}

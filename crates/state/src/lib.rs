#![forbid(unsafe_code)]

mod backend;
mod json;
mod model;
mod sqlite;

mod migrations;

pub use backend::{StateBackend, StateBackendKind, StateError, StateResult};
pub use json::JsonStateBackend;
pub use model::{
    StateSnapshot, StoredAgentEntry, StoredLogEntry, StoredThoughtEntry, CURRENT_STATE_VERSION,
    LEGACY_STATE_VERSION,
};
pub use sqlite::{
    load_state_from_sqlite_path, save_state_to_sqlite_path, sqlite_schema_version_at,
    SqliteStateBackend,
};

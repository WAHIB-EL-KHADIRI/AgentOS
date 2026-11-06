use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agentos_state::{
    JsonStateBackend, SqliteStateBackend, StateBackend, StateSnapshot, StoredAgentEntry,
    StoredLogEntry, StoredThoughtEntry, CURRENT_STATE_VERSION, LEGACY_STATE_VERSION,
};

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "agentos_state_{label}_{}_{nonce}",
        std::process::id()
    ))
}

fn populated_snapshot() -> StateSnapshot {
    StateSnapshot {
        agents: vec![StoredAgentEntry {
            agent_id: "agent-1".into(),
            name: "demo".into(),
            config_path: "demo.toml".into(),
            state: "running".into(),
            started_at_ms: 10,
            updated_at_ms: 11,
        }],
        logs: vec![StoredLogEntry {
            agent_id: "agent-1".into(),
            event_type: "started".into(),
            message: "ready".into(),
            timestamp_ms: 12,
        }],
        thoughts: vec![StoredThoughtEntry {
            checkpoint_id: "checkpoint-1".into(),
            agent_id: "agent-1".into(),
            content: "checkpoint".into(),
            timestamp_ms: 13,
            parent_checkpoint_id: None,
            metadata: Default::default(),
        }],
        ..StateSnapshot::default()
    }
}

#[test]
fn json_backend_round_trips_a_complete_snapshot() {
    let dir = temp_dir("json");
    let path = dir.join("state.json");
    let backend = JsonStateBackend::new(&path);
    let snapshot = populated_snapshot();

    backend.save_state(&snapshot).unwrap();
    let loaded = backend.load_state().unwrap();

    assert_eq!(backend.kind().as_str(), "json");
    assert_eq!(loaded, snapshot);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sqlite_backend_round_trips_a_complete_snapshot() {
    let dir = temp_dir("sqlite");
    let path = dir.join("state.sqlite");
    let backend = SqliteStateBackend::new(&path);
    let snapshot = populated_snapshot();

    backend.save_state(&snapshot).unwrap();
    let loaded = backend.load_state().unwrap();

    assert_eq!(backend.kind().as_str(), "sqlite");
    assert_eq!(loaded, snapshot);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn portable_snapshot_keeps_the_legacy_version_default() {
    let mut legacy = serde_json::to_value(populated_snapshot()).unwrap();
    legacy.as_object_mut().unwrap().remove("version");

    let decoded: StateSnapshot = serde_json::from_value(legacy).unwrap();

    assert_eq!(decoded.version, LEGACY_STATE_VERSION);
    assert_eq!(StateSnapshot::default().version, CURRENT_STATE_VERSION);
}

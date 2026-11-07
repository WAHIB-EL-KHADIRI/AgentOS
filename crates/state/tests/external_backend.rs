use std::sync::Mutex;

use agentos_state::{StateBackend, StateBackendKind, StateError, StateResult, StateSnapshot};

#[derive(Default)]
struct InMemoryBackend(Mutex<Option<StateSnapshot>>);

impl StateBackend for InMemoryBackend {
    fn kind(&self) -> StateBackendKind {
        StateBackendKind::new("test-memory")
    }

    fn load_state(&self) -> StateResult<StateSnapshot> {
        self.0
            .lock()
            .map_err(|error| StateError::message(error.to_string()))
            .map(|state| state.clone().unwrap_or_default())
    }

    fn save_state(&self, state: &StateSnapshot) -> StateResult<()> {
        *self
            .0
            .lock()
            .map_err(|error| StateError::message(error.to_string()))? = Some(state.clone());
        Ok(())
    }
}

#[test]
fn third_party_backend_can_round_trip_a_public_snapshot() {
    let backend = InMemoryBackend::default();
    let snapshot = StateSnapshot {
        agents: vec![agentos_state::StoredAgentEntry {
            agent_id: "agent-1".into(),
            name: "demo".into(),
            config_path: "demo.toml".into(),
            state: "running".into(),
            started_at_ms: 10,
            updated_at_ms: 11,
        }],
        ..StateSnapshot::default()
    };

    backend.save_state(&snapshot).unwrap();

    assert_eq!(backend.kind().as_str(), "test-memory");
    assert_eq!(backend.load_state().unwrap(), snapshot);
}

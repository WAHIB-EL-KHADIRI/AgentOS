use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use agentos_trace::RecordedThought;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

mod backend;
mod json;
pub mod migrations;
mod sqlite;

pub use backend::{StateBackend, StateBackendKind};
pub use json::JsonStateBackend;
#[cfg(test)]
pub use sqlite::save_state_to_sqlite_path;
pub use sqlite::{load_state_from_sqlite_path, sqlite_schema_version_at, SqliteStateBackend};

static STATE_BACKEND_KIND: OnceLock<Mutex<StateBackendKind>> = OnceLock::new();

fn state_backend_kind_cell() -> &'static Mutex<StateBackendKind> {
    STATE_BACKEND_KIND.get_or_init(|| Mutex::new(StateBackendKind::from_env()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLogEntry {
    pub agent_id: String,
    pub event_type: String,
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAgentEntry {
    pub agent_id: String,
    pub name: String,
    pub config_path: String,
    pub state: String,
    #[serde(default)]
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CliState {
    agents: Vec<StoredAgentEntry>,
    logs: Vec<StoredLogEntry>,
    thoughts: Vec<RecordedThought>,
}

#[derive(Debug, Clone)]
pub enum StateDoctorStatus {
    Missing,
    Healthy,
    CorruptBackedUp(PathBuf),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct StateDoctorReport {
    pub backend: String,
    pub path: PathBuf,
    pub status: StateDoctorStatus,
    pub schema_version: Option<i64>,
    pub agents: usize,
    pub logs: usize,
    pub checkpoints: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StateCleanOptions {
    pub all: bool,
    pub older_than_ms: Option<u64>,
    pub status: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default)]
pub struct StateCleanReport {
    pub matched_agents: usize,
    pub removed_agents: usize,
    pub removed_logs: usize,
    pub removed_checkpoints: usize,
    pub remaining_agents: usize,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone)]
pub struct StateExportReport {
    pub output: PathBuf,
    pub agents: usize,
    pub logs: usize,
    pub checkpoints: usize,
    pub pretty: bool,
}

#[derive(Debug, Clone)]
pub struct StateImportReport {
    pub input: PathBuf,
    pub mode: StateImportMode,
    pub dry_run: bool,
    pub imported_agents: usize,
    pub imported_logs: usize,
    pub imported_checkpoints: usize,
    pub skipped_agents: usize,
    pub backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct StateMigrationReport {
    pub from: String,
    pub to: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub dry_run: bool,
    pub agents: usize,
    pub logs: usize,
    pub checkpoints: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateInspectAgentSummary {
    pub agent_id: String,
    pub name: String,
    pub status: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateInspectReport {
    pub path: PathBuf,
    pub valid: bool,
    pub error: Option<String>,
    pub agents: usize,
    pub logs: usize,
    pub checkpoints: usize,
    pub statuses: std::collections::BTreeMap<String, usize>,
    pub oldest_agent: Option<StateInspectAgentSummary>,
    pub newest_agent: Option<StateInspectAgentSummary>,
    pub file_size_bytes: u64,
}

pub fn set_state_backend(value: &str) -> anyhow::Result<()> {
    let kind = StateBackendKind::parse(value)?;
    *state_backend_kind_cell().lock() = kind;
    Ok(())
}

pub fn current_state_backend_kind() -> StateBackendKind {
    *state_backend_kind_cell().lock()
}

pub fn current_state_backend() -> Box<dyn StateBackend> {
    backend_for_kind(current_state_backend_kind())
}

fn backend_for_kind(kind: StateBackendKind) -> Box<dyn StateBackend> {
    match kind {
        StateBackendKind::Json => Box::new(JsonStateBackend::new(default_state_path())),
        StateBackendKind::Sqlite => Box::new(SqliteStateBackend::new(default_sqlite_state_path())),
    }
}

pub fn load_state() -> anyhow::Result<CliState> {
    current_state_backend().load_state()
}

pub fn load_state_from_path(path: &Path) -> anyhow::Result<CliState> {
    match read_state(path)? {
        StateReadResult::Loaded(state) => Ok(state),
        StateReadResult::Missing => Ok(CliState::default()),
        StateReadResult::Corrupt => {
            backup_corrupt_state(path)?;
            Ok(CliState::default())
        }
    }
}

pub fn save_state(state: &CliState) -> anyhow::Result<()> {
    current_state_backend().save_state(state)
}

pub fn save_state_to_path(state: &CliState, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            anyhow::anyhow!("cannot create CLI state dir '{}': {e}", parent.display())
        })?;
    }

    let json = serde_json::to_string_pretty(state)
        .map_err(|e| anyhow::anyhow!("cannot encode CLI state: {e}"))?;
    std::fs::write(path, json)
        .map_err(|e| anyhow::anyhow!("cannot write CLI state '{}': {e}", path.display()))
}

pub fn upsert_agent(
    state: &mut CliState,
    agent_id: &str,
    name: &str,
    config_path: &str,
    status: &str,
    timestamp_ms: u64,
) {
    if let Some(agent) = state
        .agents
        .iter_mut()
        .find(|agent| agent.agent_id == agent_id)
    {
        agent.name = name.to_string();
        agent.config_path = config_path.to_string();
        agent.state = status.to_string();
        if agent.started_at_ms == 0 {
            agent.started_at_ms = timestamp_ms;
        }
        agent.updated_at_ms = timestamp_ms;
        return;
    }

    state.agents.push(StoredAgentEntry {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        config_path: config_path.to_string(),
        state: status.to_string(),
        started_at_ms: timestamp_ms,
        updated_at_ms: timestamp_ms,
    });
}

pub fn list_agents(state: &CliState) -> Vec<StoredAgentEntry> {
    state.agents.clone()
}

pub fn get_agent(state: &CliState, agent_id: &str) -> Option<StoredAgentEntry> {
    state
        .agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .cloned()
}

pub fn append_log(state: &mut CliState, agent_id: &str, mut log: StoredLogEntry) {
    log.agent_id = agent_id.to_string();
    state.logs.push(log);
}

pub fn append_checkpoint(
    state: &mut CliState,
    agent_id: &str,
    mut checkpoint: RecordedThought,
) -> String {
    checkpoint.agent_id = agent_id.to_string();
    let checkpoint_id = checkpoint.checkpoint_id.clone();
    state.thoughts.push(checkpoint);
    checkpoint_id
}

pub fn new_log_entry(
    event_type: &str,
    message: impl Into<String>,
    timestamp_ms: u64,
) -> StoredLogEntry {
    StoredLogEntry {
        agent_id: String::new(),
        event_type: event_type.to_string(),
        message: message.into(),
        timestamp_ms,
    }
}

pub fn new_checkpoint(
    agent_id: &str,
    content: impl Into<String>,
    timestamp_ms: u64,
) -> RecordedThought {
    let mut checkpoint = RecordedThought::new(agent_id, content);
    checkpoint.timestamp_ms = timestamp_ms;
    checkpoint
}

pub fn logs_for_agent(state: &CliState, agent_id: &str, limit: usize) -> Vec<StoredLogEntry> {
    let mut logs = state
        .logs
        .iter()
        .filter(|entry| entry.agent_id == agent_id)
        .cloned()
        .collect::<Vec<_>>();
    logs.sort_by_key(|entry| entry.timestamp_ms);

    if logs.len() > limit {
        logs.split_off(logs.len() - limit)
    } else {
        logs
    }
}

pub fn trace_for_agent(state: &CliState, agent_id: &str) -> Vec<RecordedThought> {
    state
        .thoughts
        .iter()
        .filter(|thought| thought.agent_id == agent_id)
        .cloned()
        .collect()
}

pub fn trace_containing_checkpoint(
    state: &CliState,
    checkpoint_id: &str,
) -> Option<Vec<RecordedThought>> {
    let agent_id = state
        .thoughts
        .iter()
        .find(|thought| thought.checkpoint_id == checkpoint_id)?
        .agent_id
        .clone();
    Some(trace_for_agent(state, &agent_id))
}

pub fn has_agent(state: &CliState, agent_id: &str) -> bool {
    get_agent(state, agent_id).is_some()
        || state.logs.iter().any(|entry| entry.agent_id == agent_id)
        || state
            .thoughts
            .iter()
            .any(|thought| thought.agent_id == agent_id)
}

pub fn logs_count_for_agent(state: &CliState, agent_id: &str) -> usize {
    state
        .logs
        .iter()
        .filter(|entry| entry.agent_id == agent_id)
        .count()
}

pub fn checkpoints_count_for_agent(state: &CliState, agent_id: &str) -> usize {
    state
        .thoughts
        .iter()
        .filter(|thought| thought.agent_id == agent_id)
        .count()
}

pub fn state_counts(state: &CliState) -> (usize, usize, usize) {
    (state.agents.len(), state.logs.len(), state.thoughts.len())
}

pub fn inspect_state(input: Option<&Path>) -> StateInspectReport {
    match input {
        Some(path) => inspect_state_at(path),
        None => current_state_backend().inspect(),
    }
}

pub(crate) fn inspect_loaded_state(backend: &dyn StateBackend) -> StateInspectReport {
    let path = backend.path();
    let file_size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    match backend.load_state() {
        Ok(state) => inspect_state_value(path, file_size_bytes, state),
        Err(e) => StateInspectReport {
            path,
            valid: false,
            error: Some(e.to_string()),
            agents: 0,
            logs: 0,
            checkpoints: 0,
            statuses: std::collections::BTreeMap::new(),
            oldest_agent: None,
            newest_agent: None,
            file_size_bytes,
        },
    }
}

pub fn inspect_state_at(path: &Path) -> StateInspectReport {
    let file_size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            return StateInspectReport {
                path: path.to_path_buf(),
                valid: false,
                error: Some(e.to_string()),
                agents: 0,
                logs: 0,
                checkpoints: 0,
                statuses: std::collections::BTreeMap::new(),
                oldest_agent: None,
                newest_agent: None,
                file_size_bytes,
            };
        }
    };

    let state: CliState = match serde_json::from_str(&content) {
        Ok(state) => state,
        Err(e) => {
            return StateInspectReport {
                path: path.to_path_buf(),
                valid: false,
                error: Some(e.to_string()),
                agents: 0,
                logs: 0,
                checkpoints: 0,
                statuses: std::collections::BTreeMap::new(),
                oldest_agent: None,
                newest_agent: None,
                file_size_bytes,
            };
        }
    };

    inspect_state_value(path.to_path_buf(), file_size_bytes, state)
}

fn inspect_state_value(path: PathBuf, file_size_bytes: u64, state: CliState) -> StateInspectReport {
    let (agents, logs, checkpoints) = state_counts(&state);
    let mut statuses = std::collections::BTreeMap::new();
    for agent in &state.agents {
        *statuses.entry(agent.state.clone()).or_insert(0) += 1;
    }

    let oldest_agent = state
        .agents
        .iter()
        .min_by_key(|agent| agent.updated_at_ms)
        .map(agent_summary);
    let newest_agent = state
        .agents
        .iter()
        .max_by_key(|agent| agent.updated_at_ms)
        .map(agent_summary);

    StateInspectReport {
        path,
        valid: true,
        error: None,
        agents,
        logs,
        checkpoints,
        statuses,
        oldest_agent,
        newest_agent,
        file_size_bytes,
    }
}

fn agent_summary(agent: &StoredAgentEntry) -> StateInspectAgentSummary {
    StateInspectAgentSummary {
        agent_id: agent.agent_id.clone(),
        name: agent.name.clone(),
        status: agent.state.clone(),
        updated_at_ms: agent.updated_at_ms,
    }
}

pub fn clean_state(options: &StateCleanOptions) -> anyhow::Result<StateCleanReport> {
    current_state_backend().clean(options)
}

pub(crate) fn clean_backend_state(
    backend: &dyn StateBackend,
    options: &StateCleanOptions,
) -> anyhow::Result<StateCleanReport> {
    let mut state = backend.load_state()?;
    let report = clean_state_in_memory(&mut state, options)?;
    if !options.dry_run && report.matched_agents > 0 {
        backend.save_state(&state)?;
    }
    Ok(report)
}

pub fn clean_state_in_memory(
    state: &mut CliState,
    options: &StateCleanOptions,
) -> anyhow::Result<StateCleanReport> {
    if !options.all && options.older_than_ms.is_none() && options.status.is_none() {
        anyhow::bail!("state clean needs --all, --older-than, or --status");
    }

    let now = current_time_millis();
    let cutoff = options.older_than_ms.map(|age| now.saturating_sub(age));
    let status_filter = options.status.as_deref();

    let matched_ids = state
        .agents
        .iter()
        .filter(|agent| {
            let matches_all = options.all;
            let matches_status = status_filter
                .map(|status| agent.state == status)
                .unwrap_or(false);
            let matches_age = cutoff
                .map(|cutoff| agent.updated_at_ms > 0 && agent.updated_at_ms <= cutoff)
                .unwrap_or(false);
            matches_all || matches_status || matches_age
        })
        .map(|agent| agent.agent_id.clone())
        .collect::<std::collections::HashSet<_>>();

    let matched_agents = matched_ids.len();
    let removed_logs = state
        .logs
        .iter()
        .filter(|entry| matched_ids.contains(&entry.agent_id))
        .count();
    let removed_checkpoints = state
        .thoughts
        .iter()
        .filter(|thought| matched_ids.contains(&thought.agent_id))
        .count();

    if !options.dry_run {
        state
            .agents
            .retain(|agent| !matched_ids.contains(&agent.agent_id));
        state
            .logs
            .retain(|entry| !matched_ids.contains(&entry.agent_id));
        state
            .thoughts
            .retain(|thought| !matched_ids.contains(&thought.agent_id));
    }

    Ok(StateCleanReport {
        matched_agents,
        removed_agents: matched_agents,
        removed_logs,
        removed_checkpoints,
        remaining_agents: state.agents.len(),
        dry_run: options.dry_run,
    })
}

pub fn export_state(output: &Path, pretty: bool) -> anyhow::Result<StateExportReport> {
    current_state_backend().export_state(output, pretty)
}

pub(crate) fn export_backend_state(
    backend: &dyn StateBackend,
    output: &Path,
    pretty: bool,
) -> anyhow::Result<StateExportReport> {
    let state = backend.load_state()?;
    export_state_to_path(&state, output, pretty)
}

pub fn migrate_state(
    from: StateBackendKind,
    to: StateBackendKind,
    dry_run: bool,
) -> anyhow::Result<StateMigrationReport> {
    if from != StateBackendKind::Json || to != StateBackendKind::Sqlite {
        anyhow::bail!("only json -> sqlite migration is supported in this phase");
    }

    backend_for_kind(to).migrate_from_json(&default_state_path(), dry_run)
}

pub fn migrate_json_to_sqlite_paths(
    source: &Path,
    target: &Path,
    dry_run: bool,
) -> anyhow::Result<StateMigrationReport> {
    let state = JsonStateBackend::new(source).load_state()?;
    let (agents, logs, checkpoints) = state_counts(&state);

    if !dry_run {
        SqliteStateBackend::new(target).save_state(&state)?;
    }

    Ok(StateMigrationReport {
        from: StateBackendKind::Json.as_str().to_string(),
        to: StateBackendKind::Sqlite.as_str().to_string(),
        source: source.to_path_buf(),
        target: target.to_path_buf(),
        dry_run,
        agents,
        logs,
        checkpoints,
    })
}

pub fn export_state_to_path(
    state: &CliState,
    output: &Path,
    pretty: bool,
) -> anyhow::Result<StateExportReport> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create export dir '{}': {e}", parent.display()))?;
    }

    let json = if pretty {
        serde_json::to_string_pretty(state)
    } else {
        serde_json::to_string(state)
    }
    .map_err(|e| anyhow::anyhow!("cannot encode CLI state export: {e}"))?;
    std::fs::write(output, json)
        .map_err(|e| anyhow::anyhow!("cannot write state export '{}': {e}", output.display()))?;

    let (agents, logs, checkpoints) = state_counts(state);
    Ok(StateExportReport {
        output: output.to_path_buf(),
        agents,
        logs,
        checkpoints,
        pretty,
    })
}

pub fn import_state(
    input: &Path,
    mode: StateImportMode,
    dry_run: bool,
) -> anyhow::Result<StateImportReport> {
    current_state_backend().import_state(input, mode, dry_run)
}

pub(crate) fn import_into_backend(
    backend: &dyn StateBackend,
    input: &Path,
    mode: StateImportMode,
    dry_run: bool,
) -> anyhow::Result<StateImportReport> {
    let imported = read_import_file(input)?;
    let current = backend.load_state()?;
    let (next_state, imported_agents, imported_logs, imported_checkpoints, skipped_agents) =
        match mode {
            StateImportMode::Merge => merge_imported_state(current, imported),
            StateImportMode::Replace => {
                let (agents, logs, checkpoints) = state_counts(&imported);
                (imported, agents, logs, checkpoints, 0)
            }
        };

    let backup_path = if dry_run {
        None
    } else {
        let backup_path = if mode == StateImportMode::Replace {
            Some(backup_current_backend_state(backend)?)
        } else {
            None
        };
        backend.save_state(&next_state)?;
        backup_path
    };

    Ok(StateImportReport {
        input: input.to_path_buf(),
        mode,
        dry_run,
        imported_agents,
        imported_logs,
        imported_checkpoints,
        skipped_agents,
        backup_path,
    })
}

#[cfg(test)]
pub fn import_state_to_path(
    input: &Path,
    state_path: &Path,
    mode: StateImportMode,
    dry_run: bool,
) -> anyhow::Result<StateImportReport> {
    let imported = read_import_file(input)?;
    let current = load_state_from_path(state_path)?;
    let (next_state, imported_agents, imported_logs, imported_checkpoints, skipped_agents) =
        match mode {
            StateImportMode::Merge => merge_imported_state(current, imported),
            StateImportMode::Replace => {
                let (agents, logs, checkpoints) = state_counts(&imported);
                (imported, agents, logs, checkpoints, 0)
            }
        };

    let backup_path = if dry_run {
        None
    } else {
        let backup_path = if mode == StateImportMode::Replace {
            Some(backup_current_state(state_path)?)
        } else {
            None
        };
        save_state_to_path(&next_state, state_path)?;
        backup_path
    };

    Ok(StateImportReport {
        input: input.to_path_buf(),
        mode,
        dry_run,
        imported_agents,
        imported_logs,
        imported_checkpoints,
        skipped_agents,
        backup_path,
    })
}

fn read_import_file(input: &Path) -> anyhow::Result<CliState> {
    let content = std::fs::read_to_string(input)
        .map_err(|e| anyhow::anyhow!("cannot read import file '{}': {e}", input.display()))?;
    serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid state import file '{}': {e}", input.display()))
}

fn merge_imported_state(
    mut current: CliState,
    imported: CliState,
) -> (CliState, usize, usize, usize, usize) {
    let existing = current
        .agents
        .iter()
        .map(|agent| agent.agent_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let new_ids = imported
        .agents
        .iter()
        .filter(|agent| !existing.contains(&agent.agent_id))
        .map(|agent| agent.agent_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let skipped_agents = imported.agents.len().saturating_sub(new_ids.len());

    current.agents.extend(
        imported
            .agents
            .into_iter()
            .filter(|agent| new_ids.contains(&agent.agent_id)),
    );
    current.logs.extend(
        imported
            .logs
            .into_iter()
            .filter(|entry| new_ids.contains(&entry.agent_id)),
    );
    current.thoughts.extend(
        imported
            .thoughts
            .into_iter()
            .filter(|thought| new_ids.contains(&thought.agent_id)),
    );

    let imported_agents = new_ids.len();
    let imported_logs = current
        .logs
        .iter()
        .filter(|entry| new_ids.contains(&entry.agent_id))
        .count();
    let imported_checkpoints = current
        .thoughts
        .iter()
        .filter(|thought| new_ids.contains(&thought.agent_id))
        .count();

    (
        current,
        imported_agents,
        imported_logs,
        imported_checkpoints,
        skipped_agents,
    )
}

pub fn parse_duration_ms(value: &str) -> anyhow::Result<u64> {
    let value = value.trim();
    if value.len() < 2 {
        anyhow::bail!("duration must use a suffix: m, h, or d");
    }

    let (amount, unit) = value.split_at(value.len() - 1);
    let amount = amount
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("duration amount must be a positive number"))?;
    let multiplier = match unit {
        "m" => 60 * 1000,
        "h" => 60 * 60 * 1000,
        "d" => 24 * 60 * 60 * 1000,
        _ => anyhow::bail!("duration suffix must be one of: m, h, d"),
    };

    amount
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!("duration is too large"))
}

pub fn doctor_state() -> anyhow::Result<StateDoctorReport> {
    current_state_backend().doctor()
}

pub fn doctor_state_at(path: &Path) -> anyhow::Result<StateDoctorReport> {
    match read_state(path)? {
        StateReadResult::Loaded(state) => {
            let (agents, logs, checkpoints) = state_counts(&state);
            Ok(StateDoctorReport {
                backend: StateBackendKind::Json.as_str().to_string(),
                path: path.to_path_buf(),
                status: StateDoctorStatus::Healthy,
                schema_version: None,
                agents,
                logs,
                checkpoints,
            })
        }
        StateReadResult::Missing => Ok(StateDoctorReport {
            backend: StateBackendKind::Json.as_str().to_string(),
            path: path.to_path_buf(),
            status: StateDoctorStatus::Missing,
            schema_version: None,
            agents: 0,
            logs: 0,
            checkpoints: 0,
        }),
        StateReadResult::Corrupt => {
            let backup = backup_corrupt_state(path)?;
            Ok(StateDoctorReport {
                backend: StateBackendKind::Json.as_str().to_string(),
                path: path.to_path_buf(),
                status: StateDoctorStatus::CorruptBackedUp(backup),
                schema_version: None,
                agents: 0,
                logs: 0,
                checkpoints: 0,
            })
        }
    }
}

pub fn doctor_sqlite_state_at(path: &Path) -> anyhow::Result<StateDoctorReport> {
    match sqlite_schema_version_at(path) {
        Ok(version) => match load_state_from_sqlite_path(path) {
            Ok(state) => {
                let (agents, logs, checkpoints) = state_counts(&state);
                Ok(StateDoctorReport {
                    backend: StateBackendKind::Sqlite.as_str().to_string(),
                    path: path.to_path_buf(),
                    status: StateDoctorStatus::Healthy,
                    schema_version: Some(version),
                    agents,
                    logs,
                    checkpoints,
                })
            }
            Err(e) => Ok(StateDoctorReport {
                backend: StateBackendKind::Sqlite.as_str().to_string(),
                path: path.to_path_buf(),
                status: StateDoctorStatus::Error(e.to_string()),
                schema_version: Some(version),
                agents: 0,
                logs: 0,
                checkpoints: 0,
            }),
        },
        Err(e) => Ok(StateDoctorReport {
            backend: StateBackendKind::Sqlite.as_str().to_string(),
            path: path.to_path_buf(),
            status: StateDoctorStatus::Error(e.to_string()),
            schema_version: None,
            agents: 0,
            logs: 0,
            checkpoints: 0,
        }),
    }
}

pub fn default_state_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".agentos")
        .join("cli-state.json")
}

pub fn default_sqlite_state_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".agentos")
        .join("agentos.sqlite")
}

pub fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

enum StateReadResult {
    Loaded(CliState),
    Missing,
    Corrupt,
}

fn read_state(path: &Path) -> anyhow::Result<StateReadResult> {
    if !path.exists() {
        return Ok(StateReadResult::Missing);
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read CLI state '{}': {e}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(StateReadResult::Loaded(CliState::default()));
    }

    match serde_json::from_str(&content) {
        Ok(state) => Ok(StateReadResult::Loaded(state)),
        Err(_) => Ok(StateReadResult::Corrupt),
    }
}

fn backup_corrupt_state(path: &Path) -> anyhow::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let backup_path = parent.join(format!("cli-state.corrupt.{}.json", current_time_millis()));
    std::fs::rename(path, &backup_path).map_err(|e| {
        anyhow::anyhow!(
            "cannot backup corrupt CLI state '{}' to '{}': {e}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(backup_path)
}

fn backup_current_state(path: &Path) -> anyhow::Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create state dir '{}': {e}", parent.display()))?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let backup_path = parent.join(format!("cli-state.backup.{}.json", current_time_millis()));
    if path.exists() {
        std::fs::copy(path, &backup_path).map_err(|e| {
            anyhow::anyhow!(
                "cannot backup current CLI state '{}' to '{}': {e}",
                path.display(),
                backup_path.display()
            )
        })?;
    } else {
        save_state_to_path(&CliState::default(), &backup_path)?;
    }
    Ok(backup_path)
}

fn backup_current_backend_state(backend: &dyn StateBackend) -> anyhow::Result<PathBuf> {
    let path = backend.path();
    let looks_like_sqlite = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("sqlite") || ext.eq_ignore_ascii_case("db"))
        .unwrap_or(false);

    if backend.kind() == StateBackendKind::Sqlite || looks_like_sqlite {
        let db_path = path;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("cannot create state backup dir '{}': {e}", parent.display())
            })?;
            let backup_path = parent.join(format!(
                "agentos.sqlite.backup.{}.json",
                current_time_millis()
            ));
            let state = backend.load_state()?;
            export_state_to_path(&state, &backup_path, true)?;
            Ok(backup_path)
        } else {
            anyhow::bail!(
                "SQLite state path '{}' has no parent directory",
                db_path.display()
            )
        }
    } else {
        backup_current_state(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn state_filters_logs_and_trace_by_agent() {
        let mut state = CliState::default();
        upsert_agent(&mut state, "agent_a", "Agent A", "a.toml", "running", 1);
        upsert_agent(&mut state, "agent_a", "Agent A", "a.toml", "stopped", 6);
        append_log(
            &mut state,
            "agent_a",
            new_log_entry("started", "Agent A started", 2),
        );
        append_log(
            &mut state,
            "agent_b",
            new_log_entry("started", "Agent B started", 3),
        );
        let checkpoint_id = append_checkpoint(
            &mut state,
            "agent_a",
            new_checkpoint("agent_a", "Agent A spawned", 4),
        );
        append_checkpoint(
            &mut state,
            "agent_b",
            new_checkpoint("agent_b", "Agent B spawned", 5),
        );

        let logs = logs_for_agent(&state, "agent_a", 10);
        let trace = trace_for_agent(&state, "agent_a");
        let replay_trace = trace_containing_checkpoint(&state, &checkpoint_id).unwrap();
        let agent = get_agent(&state, "agent_a").unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Agent A started");
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].checkpoint_id, checkpoint_id);
        assert_eq!(replay_trace.len(), 1);
        assert_eq!(agent.started_at_ms, 1);
        assert_eq!(agent.updated_at_ms, 6);
        assert_eq!(logs_count_for_agent(&state, "agent_a"), 1);
        assert_eq!(checkpoints_count_for_agent(&state, "agent_a"), 1);
    }

    #[test]
    fn state_round_trips_to_disk() {
        let path =
            std::env::temp_dir().join(format!("agentos_cli_state_{}.json", current_time_millis()));
        let mut state = CliState::default();
        upsert_agent(&mut state, "agent_a", "Agent A", "a.toml", "stopped", 1);
        append_log(
            &mut state,
            "agent_a",
            new_log_entry("stopped", "Agent A stopped", 2),
        );
        append_checkpoint(
            &mut state,
            "agent_a",
            new_checkpoint("agent_a", "Agent stopped", 3),
        );

        save_state_to_path(&state, &path).unwrap();
        let loaded = load_state_from_path(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(has_agent(&loaded, "agent_a"));
        assert_eq!(logs_for_agent(&loaded, "agent_a", 10).len(), 1);
        assert_eq!(trace_for_agent(&loaded, "agent_a").len(), 1);
        assert_eq!(list_agents(&loaded).len(), 1);
    }

    #[test]
    fn corrupt_state_is_backed_up_and_replaced_with_empty_state() {
        let dir = std::env::temp_dir().join(format!(
            "agentos_cli_state_corrupt_{}",
            current_time_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cli-state.json");
        std::fs::write(&path, "{not-json").unwrap();

        let loaded = load_state_from_path(&path).unwrap();
        let backups = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("cli-state.corrupt.")
            })
            .count();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(state_counts(&loaded), (0, 0, 0));
        assert_eq!(backups, 1);
    }

    #[test]
    fn clean_dry_run_does_not_delete() {
        let mut state = populated_clean_state();
        let options = StateCleanOptions {
            status: Some("completed".to_string()),
            dry_run: true,
            ..StateCleanOptions::default()
        };

        let report = clean_state_in_memory(&mut state, &options).unwrap();

        assert_eq!(report.matched_agents, 1);
        assert_eq!(state_counts(&state), (3, 3, 3));
    }

    #[test]
    fn clean_by_status_removes_matching_agent_data() {
        let mut state = populated_clean_state();
        let options = StateCleanOptions {
            status: Some("completed".to_string()),
            ..StateCleanOptions::default()
        };

        let report = clean_state_in_memory(&mut state, &options).unwrap();

        assert_eq!(report.removed_agents, 1);
        assert!(!has_agent(&state, "agent_completed"));
        assert_eq!(state_counts(&state), (2, 2, 2));
    }

    #[test]
    fn clean_older_than_removes_old_agent_data() {
        let mut state = populated_clean_state();
        let options = StateCleanOptions {
            older_than_ms: Some(60 * 1000),
            ..StateCleanOptions::default()
        };

        let report = clean_state_in_memory(&mut state, &options).unwrap();

        assert_eq!(report.removed_agents, 1);
        assert!(!has_agent(&state, "agent_old"));
        assert_eq!(state_counts(&state), (2, 2, 2));
    }

    #[test]
    fn clean_all_removes_everything() {
        let mut state = populated_clean_state();
        let options = StateCleanOptions {
            all: true,
            ..StateCleanOptions::default()
        };

        let report = clean_state_in_memory(&mut state, &options).unwrap();

        assert_eq!(report.removed_agents, 3);
        assert_eq!(state_counts(&state), (0, 0, 0));
    }

    #[test]
    fn parse_duration_supports_minutes_hours_and_days() {
        assert_eq!(parse_duration_ms("5m").unwrap(), 5 * 60 * 1000);
        assert_eq!(parse_duration_ms("2h").unwrap(), 2 * 60 * 60 * 1000);
        assert_eq!(parse_duration_ms("7d").unwrap(), 7 * 24 * 60 * 60 * 1000);
        assert!(parse_duration_ms("7x").is_err());
    }

    #[test]
    fn export_writes_state_file() {
        let dir = temp_state_dir("export");
        let output = dir.join("backup.json");
        let state = populated_clean_state();

        let report = export_state_to_path(&state, &output, true).unwrap();
        let loaded = load_state_from_path(&output).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.agents, 3);
        assert!(report.pretty);
        assert_eq!(state_counts(&loaded), (3, 3, 3));
    }

    #[test]
    fn import_dry_run_does_not_write() {
        let dir = temp_state_dir("import_dry_run");
        let state_path = dir.join("cli-state.json");
        let input = dir.join("backup.json");
        let current = state_with_agent("agent_existing", "running", 1);
        let incoming = state_with_agent("agent_new", "completed", 2);
        save_state_to_path(&current, &state_path).unwrap();
        save_state_to_path(&incoming, &input).unwrap();

        let report =
            import_state_to_path(&input, &state_path, StateImportMode::Merge, true).unwrap();
        let loaded = load_state_from_path(&state_path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(report.dry_run);
        assert_eq!(report.imported_agents, 1);
        assert!(has_agent(&loaded, "agent_existing"));
        assert!(!has_agent(&loaded, "agent_new"));
    }

    #[test]
    fn import_merge_adds_new_agents_only() {
        let dir = temp_state_dir("import_merge");
        let state_path = dir.join("cli-state.json");
        let input = dir.join("backup.json");
        let current = state_with_agent("agent_existing", "running", 1);
        let mut incoming = state_with_agent("agent_existing", "completed", 2);
        upsert_agent(
            &mut incoming,
            "agent_new",
            "agent_new",
            "agent_new.toml",
            "done",
            3,
        );
        append_log(&mut incoming, "agent_new", new_log_entry("done", "new", 3));
        append_checkpoint(
            &mut incoming,
            "agent_new",
            new_checkpoint("agent_new", "new", 3),
        );
        save_state_to_path(&current, &state_path).unwrap();
        save_state_to_path(&incoming, &input).unwrap();

        let report =
            import_state_to_path(&input, &state_path, StateImportMode::Merge, false).unwrap();
        let loaded = load_state_from_path(&state_path).unwrap();
        let existing = get_agent(&loaded, "agent_existing").unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.imported_agents, 1);
        assert_eq!(report.skipped_agents, 1);
        assert_eq!(existing.state, "running");
        assert!(has_agent(&loaded, "agent_new"));
    }

    #[test]
    fn import_replace_swaps_state_and_creates_backup() {
        let dir = temp_state_dir("import_replace");
        let state_path = dir.join("cli-state.json");
        let input = dir.join("backup.json");
        let current = state_with_agent("agent_existing", "running", 1);
        let incoming = state_with_agent("agent_replacement", "completed", 2);
        save_state_to_path(&current, &state_path).unwrap();
        save_state_to_path(&incoming, &input).unwrap();

        let report =
            import_state_to_path(&input, &state_path, StateImportMode::Replace, false).unwrap();
        let loaded = load_state_from_path(&state_path).unwrap();
        let backup_exists = report
            .backup_path
            .as_ref()
            .is_some_and(|path| path.exists());
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.imported_agents, 1);
        assert!(backup_exists);
        assert!(!has_agent(&loaded, "agent_existing"));
        assert!(has_agent(&loaded, "agent_replacement"));
    }

    #[test]
    fn invalid_import_file_does_not_change_current_state() {
        let dir = temp_state_dir("invalid_import");
        let state_path = dir.join("cli-state.json");
        let input = dir.join("broken.json");
        let current = state_with_agent("agent_existing", "running", 1);
        save_state_to_path(&current, &state_path).unwrap();
        std::fs::write(&input, "{broken").unwrap();

        let result = import_state_to_path(&input, &state_path, StateImportMode::Replace, false);
        let loaded = load_state_from_path(&state_path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_err());
        assert!(has_agent(&loaded, "agent_existing"));
    }

    #[test]
    fn inspect_current_state_file() {
        let dir = temp_state_dir("inspect_current");
        let path = dir.join("cli-state.json");
        let state = populated_clean_state();
        save_state_to_path(&state, &path).unwrap();

        let report = inspect_state_at(&path);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(report.valid);
        assert_eq!(report.agents, 3);
        assert_eq!(report.logs, 3);
        assert_eq!(report.checkpoints, 3);
        assert_eq!(report.statuses.get("completed"), Some(&1));
        assert!(report.oldest_agent.is_some());
        assert!(report.newest_agent.is_some());
        assert!(report.file_size_bytes > 0);
    }

    #[test]
    fn inspect_external_file() {
        let dir = temp_state_dir("inspect_external");
        let path = dir.join("backup.json");
        let state = state_with_agent("agent_external", "completed", 10);
        save_state_to_path(&state, &path).unwrap();

        let report = inspect_state(Some(&path));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(report.valid);
        assert_eq!(report.path, path);
        assert_eq!(report.agents, 1);
        assert_eq!(report.newest_agent.unwrap().agent_id, "agent_external");
    }

    #[test]
    fn inspect_invalid_file_reports_error_without_recovery() {
        let dir = temp_state_dir("inspect_invalid");
        let path = dir.join("broken.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{broken").unwrap();

        let report = inspect_state_at(&path);
        let backups = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
            .count();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!report.valid);
        assert!(report.error.is_some());
        assert_eq!(backups, 0);
    }

    #[test]
    fn inspect_report_serializes_to_json() {
        let dir = temp_state_dir("inspect_json");
        let path = dir.join("backup.json");
        let state = state_with_agent("agent_json", "running", 10);
        save_state_to_path(&state, &path).unwrap();

        let report = inspect_state_at(&path);
        let json = serde_json::to_string(&report).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(json.contains("\"valid\":true"));
        assert!(json.contains("\"agents\":1"));
        assert!(json.contains("agent_json"));
    }

    #[test]
    fn sqlite_backend_initializes_schema() {
        let dir = temp_state_dir("sqlite_init");
        let path = dir.join("agentos.sqlite");
        let backend = SqliteStateBackend::new(&path);

        backend.save_state(&CliState::default()).unwrap();
        let loaded = backend.load_state().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(state_counts(&loaded), (0, 0, 0));
    }

    #[test]
    fn sqlite_is_default_state_backend() {
        assert_eq!(StateBackendKind::from_env(), StateBackendKind::Sqlite);
    }
    #[test]
    fn sqlite_backend_insert_and_list_agents() {
        let dir = temp_state_dir("sqlite_agents");
        let path = dir.join("agentos.sqlite");
        let backend = SqliteStateBackend::new(&path);
        let state = state_with_agent("agent_sqlite", "running", 10);

        backend.save_state(&state).unwrap();
        let loaded = backend.load_state().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let agents = list_agents(&loaded);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].agent_id, "agent_sqlite");
    }

    #[test]
    fn sqlite_backend_persists_logs_and_checkpoints() {
        let dir = temp_state_dir("sqlite_logs_checkpoints");
        let path = dir.join("agentos.sqlite");
        let backend = SqliteStateBackend::new(&path);
        let state = state_with_agent("agent_sqlite", "completed", 10);

        backend.save_state(&state).unwrap();
        let loaded = backend.load_state().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(logs_for_agent(&loaded, "agent_sqlite", 10).len(), 1);
        assert_eq!(trace_for_agent(&loaded, "agent_sqlite").len(), 1);
    }

    #[test]
    fn json_to_sqlite_migration_dry_run_does_not_create_sqlite() {
        let dir = temp_state_dir("migration_dry_run");
        let json = dir.join("cli-state.json");
        let sqlite = dir.join("agentos.sqlite");
        let state = populated_clean_state();
        save_state_to_path(&state, &json).unwrap();

        let report = migrate_json_to_sqlite_paths(&json, &sqlite, true).unwrap();
        let sqlite_exists = sqlite.exists();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(report.dry_run);
        assert_eq!(report.agents, 3);
        assert!(!sqlite_exists);
    }

    #[test]
    fn json_to_sqlite_migration_writes_sqlite() {
        let dir = temp_state_dir("migration_apply");
        let json = dir.join("cli-state.json");
        let sqlite = dir.join("agentos.sqlite");
        let state = populated_clean_state();
        save_state_to_path(&state, &json).unwrap();

        let report = migrate_json_to_sqlite_paths(&json, &sqlite, false).unwrap();
        let loaded = load_state_from_sqlite_path(&sqlite).unwrap();
        let json_still_exists = json.exists();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!report.dry_run);
        assert_eq!(state_counts(&loaded), (3, 3, 3));
        assert!(json_still_exists);
    }

    #[test]
    fn sqlite_schema_migration_v1_is_recorded() {
        let dir = temp_state_dir("sqlite_schema_v1");
        let path = dir.join("agentos.sqlite");

        save_state_to_sqlite_path(&CliState::default(), &path).unwrap();
        let version = sqlite_schema_version_at(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(version, 1);
    }

    #[test]
    fn sqlite_future_schema_version_errors() {
        let dir = temp_state_dir("sqlite_future_schema");
        let path = dir.join("agentos.sqlite");
        std::fs::create_dir_all(&dir).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at_ms INTEGER NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at_ms) VALUES (99, 1)",
            [],
        )
        .unwrap();
        drop(conn);

        let result = load_state_from_sqlite_path(&path);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("newer than supported"));
    }

    #[test]
    fn sqlite_doctor_reports_healthy_counts() {
        let dir = temp_state_dir("sqlite_doctor");
        let path = dir.join("agentos.sqlite");
        let state = populated_clean_state();
        save_state_to_sqlite_path(&state, &path).unwrap();

        let report = doctor_sqlite_state_at(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.backend, "sqlite");
        assert!(matches!(report.status, StateDoctorStatus::Healthy));
        assert_eq!(report.schema_version, Some(1));
        assert_eq!((report.agents, report.logs, report.checkpoints), (3, 3, 3));
    }

    #[test]
    fn sqlite_export_writes_portable_json() {
        let dir = temp_state_dir("sqlite_export");
        let sqlite = dir.join("agentos.sqlite");
        let output = dir.join("sqlite-backup.json");
        let state = populated_clean_state();
        save_state_to_sqlite_path(&state, &sqlite).unwrap();
        let loaded = load_state_from_sqlite_path(&sqlite).unwrap();

        let report = export_state_to_path(&loaded, &output, true).unwrap();
        let exported = load_state_from_path(&output).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.agents, 3);
        assert_eq!(state_counts(&exported), (3, 3, 3));
    }

    #[test]
    fn sqlite_import_dry_run_does_not_write() {
        let dir = temp_state_dir("sqlite_import_dry_run");
        let sqlite = dir.join("agentos.sqlite");
        let input = dir.join("backup.json");
        let current = state_with_agent("agent_existing", "running", 1);
        let incoming = state_with_agent("agent_new", "completed", 2);
        save_state_to_sqlite_path(&current, &sqlite).unwrap();
        save_state_to_path(&incoming, &input).unwrap();

        let current_db = SqliteStateBackend::new(&sqlite);
        let imported = read_import_file(&input).unwrap();
        let (next_state, imported_agents, _, _, _) =
            merge_imported_state(current_db.load_state().unwrap(), imported);
        let loaded_after = current_db.load_state().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(imported_agents, 1);
        assert!(has_agent(&next_state, "agent_new"));
        assert!(!has_agent(&loaded_after, "agent_new"));
    }

    #[test]
    fn sqlite_import_merge_adds_new_agent() {
        let dir = temp_state_dir("sqlite_import_merge");
        let sqlite = dir.join("agentos.sqlite");
        let input = dir.join("backup.json");
        let current = state_with_agent("agent_existing", "running", 1);
        let incoming = state_with_agent("agent_new", "completed", 2);
        save_state_to_sqlite_path(&current, &sqlite).unwrap();
        save_state_to_path(&incoming, &input).unwrap();

        let imported = read_import_file(&input).unwrap();
        let backend = SqliteStateBackend::new(&sqlite);
        let (next_state, imported_agents, _, _, skipped_agents) =
            merge_imported_state(backend.load_state().unwrap(), imported);
        backend.save_state(&next_state).unwrap();
        let loaded = backend.load_state().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(imported_agents, 1);
        assert_eq!(skipped_agents, 0);
        assert!(has_agent(&loaded, "agent_existing"));
        assert!(has_agent(&loaded, "agent_new"));
    }

    #[test]
    fn sqlite_import_replace_creates_json_backup() {
        let dir = temp_state_dir("sqlite_import_replace");
        let sqlite = dir.join("agentos.sqlite");
        let current = state_with_agent("agent_existing", "running", 1);
        let replacement = state_with_agent("agent_replacement", "completed", 2);
        save_state_to_sqlite_path(&current, &sqlite).unwrap();
        let backend = SqliteStateBackend::new(&sqlite);

        let backup = backup_current_backend_state(&backend).unwrap();
        backend.save_state(&replacement).unwrap();
        let loaded = backend.load_state().unwrap();
        let backup_state = load_state_from_path(&backup).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(backup.to_string_lossy().ends_with(".json"));
        assert!(has_agent(&backup_state, "agent_existing"));
        assert!(!has_agent(&loaded, "agent_existing"));
        assert!(has_agent(&loaded, "agent_replacement"));
    }

    fn populated_clean_state() -> CliState {
        let mut state = CliState::default();
        let now = current_time_millis();
        let old = now.saturating_sub(2 * 60 * 1000);

        upsert_agent(
            &mut state,
            "agent_completed",
            "Completed",
            "completed.toml",
            "completed",
            now,
        );
        upsert_agent(
            &mut state,
            "agent_running",
            "Running",
            "running.toml",
            "running",
            now,
        );
        upsert_agent(&mut state, "agent_old", "Old", "old.toml", "stopped", old);

        append_log(
            &mut state,
            "agent_completed",
            new_log_entry("completed", "done", now),
        );
        append_log(
            &mut state,
            "agent_running",
            new_log_entry("running", "live", now),
        );
        append_log(
            &mut state,
            "agent_old",
            new_log_entry("stopped", "old", old),
        );

        append_checkpoint(
            &mut state,
            "agent_completed",
            new_checkpoint("agent_completed", "done", now),
        );
        append_checkpoint(
            &mut state,
            "agent_running",
            new_checkpoint("agent_running", "live", now),
        );
        append_checkpoint(
            &mut state,
            "agent_old",
            new_checkpoint("agent_old", "old", old),
        );

        state
    }

    fn state_with_agent(agent_id: &str, status: &str, timestamp_ms: u64) -> CliState {
        let mut state = CliState::default();
        upsert_agent(
            &mut state,
            agent_id,
            agent_id,
            &format!("{agent_id}.toml"),
            status,
            timestamp_ms,
        );
        append_log(
            &mut state,
            agent_id,
            new_log_entry(status, format!("{agent_id} {status}"), timestamp_ms),
        );
        append_checkpoint(
            &mut state,
            agent_id,
            new_checkpoint(agent_id, format!("{agent_id} checkpoint"), timestamp_ms),
        );
        state
    }

    fn temp_state_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "agentos_cli_state_{name}_{}",
            current_time_millis()
        ))
    }
}

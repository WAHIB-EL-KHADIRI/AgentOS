use std::path::{Path, PathBuf};

use crate::state::{
    CliState, StateCleanOptions, StateCleanReport, StateDoctorReport, StateExportReport,
    StateImportMode, StateImportReport, StateInspectReport, StateMigrationReport,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBackendKind {
    Json,
    Sqlite,
}

impl StateBackendKind {
    pub fn from_env() -> Self {
        std::env::var("AGENTOS_STATE_BACKEND")
            .ok()
            .and_then(|value| Self::parse(&value).ok())
            .unwrap_or(Self::Json)
    }

    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "sqlite" => Ok(Self::Sqlite),
            other => anyhow::bail!("unsupported state backend '{other}'. Use: json, sqlite"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Sqlite => "sqlite",
        }
    }
}

pub trait StateBackend {
    fn kind(&self) -> StateBackendKind;
    fn path(&self) -> PathBuf;
    fn load_state(&self) -> anyhow::Result<CliState>;
    fn save_state(&self, state: &CliState) -> anyhow::Result<()>;
    fn doctor(&self) -> anyhow::Result<StateDoctorReport>;
    fn inspect(&self) -> StateInspectReport;
    fn export_state(&self, output: &Path, pretty: bool) -> anyhow::Result<StateExportReport>;
    fn import_state(
        &self,
        input: &Path,
        mode: StateImportMode,
        dry_run: bool,
    ) -> anyhow::Result<StateImportReport>;
    fn clean(&self, options: &StateCleanOptions) -> anyhow::Result<StateCleanReport>;

    fn migrate_from_json(
        &self,
        source: &Path,
        dry_run: bool,
    ) -> anyhow::Result<StateMigrationReport>;
}

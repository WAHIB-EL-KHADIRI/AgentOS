use std::path::{Path, PathBuf};

use crate::state::{
    clean_backend_state, doctor_state_at, export_backend_state, import_into_backend,
    inspect_state_at, CliState, StateBackend, StateBackendKind, StateCleanOptions,
    StateCleanReport, StateDoctorReport, StateExportReport, StateImportMode, StateImportReport,
    StateInspectReport, StateMigrationReport,
};

pub struct JsonStateBackend {
    path: PathBuf,
}

impl JsonStateBackend {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl StateBackend for JsonStateBackend {
    fn kind(&self) -> StateBackendKind {
        StateBackendKind::Json
    }

    fn path(&self) -> PathBuf {
        self.path.clone()
    }

    fn load_state(&self) -> anyhow::Result<CliState> {
        crate::state::load_state_from_path(&self.path)
    }

    fn save_state(&self, state: &CliState) -> anyhow::Result<()> {
        crate::state::save_state_to_path(state, &self.path)
    }

    fn doctor(&self) -> anyhow::Result<StateDoctorReport> {
        doctor_state_at(&self.path)
    }

    fn inspect(&self) -> StateInspectReport {
        inspect_state_at(&self.path)
    }

    fn export_state(&self, output: &Path, pretty: bool) -> anyhow::Result<StateExportReport> {
        export_backend_state(self, output, pretty)
    }

    fn import_state(
        &self,
        input: &Path,
        mode: StateImportMode,
        dry_run: bool,
    ) -> anyhow::Result<StateImportReport> {
        import_into_backend(self, input, mode, dry_run)
    }

    fn clean(&self, options: &StateCleanOptions) -> anyhow::Result<StateCleanReport> {
        clean_backend_state(self, options)
    }

    fn migrate_from_json(
        &self,
        _source: &Path,
        _dry_run: bool,
    ) -> anyhow::Result<StateMigrationReport> {
        anyhow::bail!("json -> json migration is not supported")
    }
}

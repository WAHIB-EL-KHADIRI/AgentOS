use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use thiserror::Error;

use crate::StateSnapshot;

/// A stable identity for a state backend.
///
/// This is an open string identity so external backends can name themselves.
/// Keep identifiers stable once they are used in logs, configuration, or APIs.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StateBackendKind(Cow<'static, str>);

impl StateBackendKind {
    /// The built-in portable JSON backend.
    pub const JSON: Self = Self(Cow::Borrowed("json"));

    /// The built-in SQLite backend.
    pub const SQLITE: Self = Self(Cow::Borrowed("sqlite"));

    /// Create a backend identity. Use a stable, non-empty identifier.
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    /// Return the backend's stable identifier.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for StateBackendKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Errors returned by the state model and built-in or custom backends.
#[derive(Debug, Error)]
pub enum StateError {
    #[error("could not read or write state at '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid state data: {source}")]
    InvalidData {
        #[source]
        source: serde_json::Error,
    },
    #[error("state export version {found} is newer than supported version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("state backend error: {context}: {source}")]
    Backend {
        context: String,
        #[source]
        source: Box<dyn Error + Send + Sync + 'static>,
    },
    #[error("state backend error: {0}")]
    Message(String),
}

impl StateError {
    /// Wrap an implementation-specific error without requiring `anyhow`.
    pub fn backend(context: impl Into<String>, source: impl Error + Send + Sync + 'static) -> Self {
        Self::Backend {
            context: context.into(),
            source: Box::new(source),
        }
    }

    /// Construct an error when an implementation has no underlying error value.
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn invalid_data(source: serde_json::Error) -> Self {
        Self::InvalidData { source }
    }
}

impl From<serde_json::Error> for StateError {
    fn from(source: serde_json::Error) -> Self {
        Self::invalid_data(source)
    }
}

impl From<rusqlite::Error> for StateError {
    fn from(source: rusqlite::Error) -> Self {
        Self::backend("SQLite operation failed", source)
    }
}

/// Result type used by the public state API.
pub type StateResult<T> = Result<T, StateError>;

/// Storage for complete operational state snapshots.
///
/// Reads and writes are synchronous and operate on the entire snapshot. Callers
/// must serialize read-modify-write operations; this contract does not provide
/// revision checks or protection against stale concurrent writers.
pub trait StateBackend: Send + Sync {
    /// Return this backend's stable identity.
    fn kind(&self) -> StateBackendKind;

    /// Load the current complete snapshot.
    fn load_state(&self) -> StateResult<StateSnapshot>;

    /// Replace the current complete snapshot.
    fn save_state(&self, state: &StateSnapshot) -> StateResult<()>;
}

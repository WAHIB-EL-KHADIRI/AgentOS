use thiserror::Error;

pub type AgentResult<T> = Result<T, AgentError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentError {
    #[error("agent '{0}' is already running")]
    AlreadyRunning(String),

    #[error("agent '{0}' is not running")]
    NotRunning(String),

    #[error("agent '{0}' was not found")]
    NotFound(String),

    #[error("invalid agent state: {0}")]
    InvalidState(String),

    #[error("agent command failed: {0}")]
    CommandFailed(String),

    #[error("channel closed for agent '{0}'")]
    ChannelClosed(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("timeout waiting for agent '{0}'")]
    Timeout(String),

    #[error("internal error: {0}")]
    Internal(String),
}

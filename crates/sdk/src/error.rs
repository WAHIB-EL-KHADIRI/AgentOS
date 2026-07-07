use thiserror::Error;

pub type SdkResult<T> = Result<T, SdkError>;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error(transparent)]
    Bus(#[from] agentos_bus::BusError),

    #[error(transparent)]
    Kernel(#[from] agentos_kernel::AgentError),

    #[error("failed to serialize payload: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("agent context has no bus connection configured")]
    BusNotConnected,
}

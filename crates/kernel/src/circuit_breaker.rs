use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::warn;

#[derive(Debug)]
pub enum CallError<E = String> {
    CircuitOpen(String),
    Inner(E),
}

impl<E: fmt::Display> fmt::Display for CallError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::CircuitOpen(msg) => write!(f, "circuit open: {msg}"),
            CallError::Inner(e) => write!(f, "inner error: {e}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for CallError<E> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u64,
    pub success_threshold: u64,
    pub timeout: Duration,
    pub half_open_max_calls: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    name: String,
    state: Arc<RwLock<CircuitState>>,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_config(name, CircuitBreakerConfig::default())
    }

    pub fn with_config(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure_time: Arc::new(RwLock::new(None)),
            config,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }

    pub async fn is_callable(&self) -> bool {
        match *self.state.read().await {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => {
                self.success_count.load(Ordering::Relaxed) < self.config.half_open_max_calls
            }
            CircuitState::Open => false,
        }
    }

    pub async fn call<F, T, E>(&self, f: F) -> Result<T, CallError<E>>
    where
        F: std::future::IntoFuture<Output = Result<T, E>>,
    {
        let current_state = self.state().await;

        match current_state {
            CircuitState::Open => {
                let last_failure = *self.last_failure_time.read().await;
                if let Some(time) = last_failure {
                    if time.elapsed() >= self.config.timeout {
                        *self.state.write().await = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::Relaxed);
                        warn!(
                            circuit = %self.name,
                            "circuit transitioning from OPEN to HALF_OPEN after timeout"
                        );
                    } else {
                        return Err(CallError::CircuitOpen(
                            "circuit breaker is OPEN".to_string(),
                        ));
                    }
                } else {
                    return Err(CallError::CircuitOpen(
                        "circuit breaker is OPEN".to_string(),
                    ));
                }
            }
            CircuitState::HalfOpen => {
                if self.success_count.load(Ordering::Relaxed) >= self.config.half_open_max_calls {
                    return Err(CallError::CircuitOpen(
                        "circuit breaker is half-open, max test calls reached".to_string(),
                    ));
                }
            }
            CircuitState::Closed => {}
        }

        match f.await {
            Ok(val) => {
                self.on_success().await;
                Ok(val)
            }
            Err(e) => {
                self.on_failure().await;
                Err(CallError::Inner(e))
            }
        }
    }

    async fn on_success(&self) {
        let state = self.state().await;
        match state {
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if successes >= self.config.success_threshold {
                    *self.state.write().await = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    warn!(
                        circuit = %self.name,
                        "circuit transitioning from HALF_OPEN to CLOSED after {successes} successes"
                    );
                }
            }
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
            }
            CircuitState::Open => {}
        }
    }

    async fn on_failure(&self) {
        let state = self.state().await;
        match state {
            CircuitState::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= self.config.failure_threshold {
                    *self.state.write().await = CircuitState::Open;
                    *self.last_failure_time.write().await = Some(Instant::now());
                    warn!(
                        circuit = %self.name,
                        "circuit transitioning from CLOSED to OPEN after {failures} failures"
                    );
                }
            }
            CircuitState::HalfOpen => {
                *self.state.write().await = CircuitState::Open;
                *self.last_failure_time.write().await = Some(Instant::now());
                self.success_count.store(0, Ordering::Relaxed);
                warn!(
                    circuit = %self.name,
                    "circuit transitioning from HALF_OPEN to OPEN after failure"
                );
            }
            CircuitState::Open => {
                *self.last_failure_time.write().await = Some(Instant::now());
            }
        }
    }

    pub async fn reset(&self) {
        *self.state.write().await = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        *self.last_failure_time.write().await = None;
    }
}

/// Retry with exponential backoff
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut f: F,
    max_retries: u32,
    base_delay: Duration,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::IntoFuture<Output = Result<T, E>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(e);
                }
                let delay = base_delay * 2u32.pow(attempt - 1);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new("test");
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.is_callable().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config("test", config);

        for _ in 0..3 {
            let _: Result<(), _> = cb.call(async { Err::<(), String>("fail".into()) }).await;
        }

        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.is_callable().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_rejects_when_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_secs(60),
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config("test", config);
        let _: Result<(), _> = cb.call(async { Err::<(), String>("fail".into()) }).await;

        let result: Result<(), _> = cb.call(async { Err::<(), String>("fail".into()) }).await;
        assert!(result.is_err());
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_on_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config("test", config);

        let _: Result<(), _> = cb.call(async { Err::<(), String>("fail".into()) }).await;
        assert_eq!(cb.state().await, CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let result: Result<&str, _> = cb.call(async { Ok::<_, String>("ok") }).await;
        assert!(result.is_ok());
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config("test", config);

        let _: Result<(), _> = cb.call(async { Err::<(), String>("fail".into()) }).await;
        assert_eq!(cb.state().await, CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(100)).await;

        cb.call(async { Ok::<_, String>("ok") }).await.unwrap();
        cb.call(async { Ok::<_, String>("ok") }).await.unwrap();

        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_succeeds_eventually() {
        let mut attempts = 0u32;
        let result: Result<String, String> = retry_with_backoff(
            || {
                attempts += 1;
                async move {
                    if attempts < 3 {
                        Err("not yet".to_string())
                    } else {
                        Ok("success".to_string())
                    }
                }
            },
            5,
            Duration::from_millis(10),
        )
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts, 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_exhausts_retries() {
        let result: Result<(), String> = retry_with_backoff(
            || async { Err("always fail".to_string()) },
            2,
            Duration::from_millis(5),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let cb = CircuitBreaker::with_config("test", config);

        let _: Result<(), _> = cb.call(async { Err::<(), String>("fail".into()) }).await;
        assert_eq!(cb.state().await, CircuitState::Open);

        cb.reset().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.is_callable().await);
    }
}

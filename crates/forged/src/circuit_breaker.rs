//! Circuit breaker pattern for graceful degradation.
//!
//! Protects against cascading failures by wrapping fallible operations
//! and short-circuiting when a backend is known to be unhealthy.
//!
//! States:
//! - **Closed** — requests flow through normally. Failures are counted.
//! - **Open** — requests are rejected immediately. After `recovery_timeout`
//!   the breaker transitions to HalfOpen.
//! - **HalfOpen** — a single probe request is allowed through. Success
//!   resets the breaker to Closed; failure sends it back to Open.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Current state of the circuit breaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Normal operation — calls pass through.
    Closed,
    /// Backend considered unhealthy — calls are rejected immediately.
    Open {
        /// When the breaker entered the Open state.
        since: Instant,
    },
    /// Recovery probe — one call is allowed through to test the backend.
    HalfOpen,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Closed => write!(f, "Closed"),
            State::Open { .. } => write!(f, "Open"),
            State::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// Configuration for a [`CircuitBreaker`].
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit.
    pub failure_threshold: u32,
    /// How long to stay in the Open state before allowing a probe.
    pub recovery_timeout: Duration,
    /// Human-readable name used in log messages.
    pub name: String,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            name: "default".to_string(),
        }
    }
}

/// Internal mutable state behind the lock.
#[derive(Debug)]
struct Inner {
    state: State,
    failure_count: u32,
    config: CircuitBreakerConfig,
}

/// A thread-safe, async-aware circuit breaker.
///
/// Wrap unreliable calls with [`CircuitBreaker::call`] to automatically
/// track failures and short-circuit when a backend is unhealthy.
///
/// ```ignore
/// let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
///
/// let result = cb.call(|| async {
///     some_remote_call().await
/// }).await;
/// ```
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    inner: Arc<Mutex<Inner>>,
}

/// Error returned when the circuit is open and calls are being rejected.
#[derive(Debug, Clone)]
pub struct CircuitOpenError {
    pub name: String,
    pub recovery_timeout: Duration,
}

impl fmt::Display for CircuitOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "circuit breaker '{}' is open — requests are being rejected \
             (recovery in {:?})",
            self.name, self.recovery_timeout
        )
    }
}

impl std::error::Error for CircuitOpenError {}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: State::Closed,
                failure_count: 0,
                config,
            })),
        }
    }

    /// Return the current state of the circuit breaker.
    pub async fn state(&self) -> State {
        self.inner.lock().await.state.clone()
    }

    /// Execute an async operation through the circuit breaker.
    ///
    /// Returns `Err(CircuitOpenError)` (mapped into `E`) when the circuit is
    /// open. Otherwise the result of `f` is returned and the breaker state
    /// is updated based on success / failure.
    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: From<CircuitOpenError>,
    {
        // --- pre-call check ---
        {
            let mut inner = self.inner.lock().await;
            match &inner.state {
                State::Closed => { /* allow */ }
                State::Open { since } => {
                    if since.elapsed() >= inner.config.recovery_timeout {
                        tracing::info!(
                            breaker = %inner.config.name,
                            "Circuit breaker transitioning from Open to HalfOpen"
                        );
                        inner.state = State::HalfOpen;
                        // fall through — allow the probe call
                    } else {
                        tracing::debug!(
                            breaker = %inner.config.name,
                            "Circuit breaker is Open — rejecting call"
                        );
                        return Err(CircuitOpenError {
                            name: inner.config.name.clone(),
                            recovery_timeout: inner.config.recovery_timeout,
                        }
                        .into());
                    }
                }
                State::HalfOpen => {
                    // Another probe is already in flight — reject to avoid
                    // hammering the backend.
                    return Err(CircuitOpenError {
                        name: inner.config.name.clone(),
                        recovery_timeout: inner.config.recovery_timeout,
                    }
                    .into());
                }
            }
        }
        // Lock is released here so the actual call runs without holding it.

        // --- execute the operation ---
        let result = f().await;

        // --- post-call bookkeeping ---
        {
            let mut inner = self.inner.lock().await;
            match &result {
                Ok(_) => {
                    if inner.state == State::HalfOpen {
                        tracing::info!(
                            breaker = %inner.config.name,
                            "Probe succeeded — circuit breaker returning to Closed"
                        );
                    }
                    inner.failure_count = 0;
                    inner.state = State::Closed;
                }
                Err(_) => {
                    inner.failure_count += 1;
                    let count = inner.failure_count;
                    let threshold = inner.config.failure_threshold;

                    match &inner.state {
                        State::HalfOpen => {
                            tracing::warn!(
                                breaker = %inner.config.name,
                                "Probe failed — circuit breaker returning to Open"
                            );
                            inner.state = State::Open {
                                since: Instant::now(),
                            };
                            inner.failure_count = threshold; // keep it saturated
                        }
                        State::Closed if count >= threshold => {
                            tracing::warn!(
                                breaker = %inner.config.name,
                                failures = count,
                                threshold = threshold,
                                "Failure threshold reached — opening circuit breaker"
                            );
                            inner.state = State::Open {
                                since: Instant::now(),
                            };
                        }
                        _ => {
                            tracing::debug!(
                                breaker = %inner.config.name,
                                failures = count,
                                threshold = threshold,
                                "Failure recorded"
                            );
                        }
                    }
                }
            }
        }

        result
    }

    /// Manually reset the circuit breaker to the Closed state.
    pub async fn reset(&self) {
        let mut inner = self.inner.lock().await;
        tracing::info!(breaker = %inner.config.name, "Circuit breaker manually reset");
        inner.failure_count = 0;
        inner.state = State::Closed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_millis(100),
            name: "test".to_string(),
        }
    }

    #[derive(Debug)]
    enum TestError {
        Backend(String),
        CircuitOpen(CircuitOpenError),
    }

    impl From<CircuitOpenError> for TestError {
        fn from(e: CircuitOpenError) -> Self {
            TestError::CircuitOpen(e)
        }
    }

    #[tokio::test]
    async fn closed_passes_through() {
        let cb = CircuitBreaker::new(test_config());

        let result: Result<i32, TestError> = cb.call(|| async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(cb.state().await, State::Closed);
    }

    #[tokio::test]
    async fn opens_after_threshold() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            let _: Result<i32, TestError> = cb
                .call(|| async { Err(TestError::Backend("fail".into())) })
                .await;
        }

        assert!(matches!(cb.state().await, State::Open { .. }));

        // Next call should be rejected immediately.
        let result: Result<i32, TestError> = cb.call(|| async { Ok(1) }).await;
        assert!(matches!(result, Err(TestError::CircuitOpen(_))));
    }

    #[tokio::test]
    async fn recovers_after_timeout() {
        let cb = CircuitBreaker::new(test_config());

        // Trip the breaker.
        for _ in 0..3 {
            let _: Result<i32, TestError> = cb
                .call(|| async { Err(TestError::Backend("fail".into())) })
                .await;
        }
        assert!(matches!(cb.state().await, State::Open { .. }));

        // Wait for recovery timeout.
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Probe should go through and succeed.
        let result: Result<i32, TestError> = cb.call(|| async { Ok(99) }).await;
        assert_eq!(result.unwrap(), 99);
        assert_eq!(cb.state().await, State::Closed);
    }

    #[tokio::test]
    async fn halfopen_failure_reopens() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            let _: Result<i32, TestError> = cb
                .call(|| async { Err(TestError::Backend("fail".into())) })
                .await;
        }

        tokio::time::sleep(Duration::from_millis(150)).await;

        // Probe fails — should go back to Open.
        let _: Result<i32, TestError> = cb
            .call(|| async { Err(TestError::Backend("still broken".into())) })
            .await;
        assert!(matches!(cb.state().await, State::Open { .. }));
    }

    #[tokio::test]
    async fn manual_reset() {
        let cb = CircuitBreaker::new(test_config());

        for _ in 0..3 {
            let _: Result<i32, TestError> = cb
                .call(|| async { Err(TestError::Backend("fail".into())) })
                .await;
        }
        assert!(matches!(cb.state().await, State::Open { .. }));

        cb.reset().await;
        assert_eq!(cb.state().await, State::Closed);
    }
}

use super::types::{CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Circuit breaker for a single backend
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Configuration
    config: CircuitBreakerConfig,
    /// Current state
    state: Arc<RwLock<State>>,
    /// Backend identifier
    backend: String,
}

#[derive(Debug)]
struct State {
    /// Current circuit state
    circuit_state: CircuitState,
    /// Number of consecutive failures in closed state
    consecutive_failures: u32,
    /// Number of consecutive successes in half-open state
    consecutive_successes: u32,
    /// Number of half-open requests in flight
    half_open_requests: u32,
    /// Time when the circuit was opened
    opened_at: Option<Instant>,
    /// Metrics
    metrics: CircuitBreakerMetrics,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(backend: String, config: CircuitBreakerConfig) -> Self {
        info!(
            backend = %backend,
            failure_threshold = config.failure_threshold,
            success_threshold = config.success_threshold,
            timeout_secs = config.timeout_secs,
            "Creating circuit breaker"
        );

        Self {
            config,
            state: Arc::new(RwLock::new(State {
                circuit_state: CircuitState::Closed,
                consecutive_failures: 0,
                consecutive_successes: 0,
                half_open_requests: 0,
                opened_at: None,
                metrics: CircuitBreakerMetrics::default(),
            })),
            backend,
        }
    }

    /// Check if a request can proceed
    pub async fn can_proceed(&self) -> bool {
        let mut state = self.state.write().await;

        match state.circuit_state {
            CircuitState::Closed => {
                state.metrics.total_requests += 1;
                true
            }
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed() >= self.config.timeout() {
                        // Transition to half-open
                        self.transition_to_half_open(&mut state);
                        state.metrics.total_requests += 1;
                        state.half_open_requests += 1;
                        true
                    } else {
                        // Circuit still open, reject request
                        state.metrics.rejected_requests += 1;
                        debug!(
                            backend = %self.backend,
                            time_remaining = ?self.config.timeout() - opened_at.elapsed(),
                            "Circuit breaker open, rejecting request"
                        );
                        false
                    }
                } else {
                    // Should not happen, but handle gracefully
                    warn!(backend = %self.backend, "Circuit open but no opened_at timestamp");
                    false
                }
            }
            CircuitState::HalfOpen => {
                if state.half_open_requests < self.config.half_open_requests {
                    state.metrics.total_requests += 1;
                    state.half_open_requests += 1;
                    debug!(
                        backend = %self.backend,
                        half_open_requests = state.half_open_requests,
                        max = self.config.half_open_requests,
                        "Allowing half-open probe request"
                    );
                    true
                } else {
                    // Already at max half-open requests
                    state.metrics.rejected_requests += 1;
                    debug!(
                        backend = %self.backend,
                        "Max half-open requests reached, rejecting"
                    );
                    false
                }
            }
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;
        state.metrics.successful_requests += 1;

        match state.circuit_state {
            CircuitState::Closed => {
                // Reset failure count on success
                state.consecutive_failures = 0;
            }
            CircuitState::HalfOpen => {
                state.consecutive_successes += 1;
                state.half_open_requests = state.half_open_requests.saturating_sub(1);

                debug!(
                    backend = %self.backend,
                    consecutive_successes = state.consecutive_successes,
                    threshold = self.config.success_threshold,
                    "Half-open probe request succeeded"
                );

                // Check if we should close the circuit
                if state.consecutive_successes >= self.config.success_threshold {
                    self.transition_to_closed(&mut state);
                }
            }
            CircuitState::Open => {
                // Should not happen, but handle gracefully
                warn!(backend = %self.backend, "Recording success in open state");
            }
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        state.metrics.failed_requests += 1;

        match state.circuit_state {
            CircuitState::Closed => {
                state.consecutive_failures += 1;

                debug!(
                    backend = %self.backend,
                    consecutive_failures = state.consecutive_failures,
                    threshold = self.config.failure_threshold,
                    "Request failed in closed state"
                );

                // Check if we should open the circuit
                if state.consecutive_failures >= self.config.failure_threshold {
                    self.transition_to_open(&mut state);
                }
            }
            CircuitState::HalfOpen => {
                state.half_open_requests = state.half_open_requests.saturating_sub(1);
                warn!(
                    backend = %self.backend,
                    "Half-open probe request failed, reopening circuit"
                );
                // Any failure in half-open state reopens the circuit
                self.transition_to_open(&mut state);
            }
            CircuitState::Open => {
                // Should not happen, but handle gracefully
                debug!(backend = %self.backend, "Recording failure in open state");
            }
        }
    }

    /// Record a timeout
    pub async fn record_timeout(&self) {
        let mut state = self.state.write().await;
        state.metrics.timeout_count += 1;
        drop(state);
        // Treat timeout as failure
        self.record_failure().await;
    }

    /// Get current state
    pub async fn state(&self) -> CircuitState {
        self.state.read().await.circuit_state
    }

    /// Get metrics
    pub async fn metrics(&self) -> CircuitBreakerMetrics {
        self.state.read().await.metrics.clone()
    }

    /// Transition to open state
    fn transition_to_open(&self, state: &mut State) {
        info!(
            backend = %self.backend,
            consecutive_failures = state.consecutive_failures,
            "Circuit breaker opening"
        );

        state.circuit_state = CircuitState::Open;
        state.opened_at = Some(Instant::now());
        state.consecutive_failures = 0;
        state.consecutive_successes = 0;
        state.half_open_requests = 0;
        state.metrics.circuit_opened_count += 1;
    }

    /// Transition to half-open state
    fn transition_to_half_open(&self, state: &mut State) {
        info!(
            backend = %self.backend,
            timeout = ?self.config.timeout(),
            "Circuit breaker transitioning to half-open"
        );

        state.circuit_state = CircuitState::HalfOpen;
        state.consecutive_failures = 0;
        state.consecutive_successes = 0;
        state.half_open_requests = 0;
        state.metrics.circuit_half_opened_count += 1;
    }

    /// Transition to closed state
    fn transition_to_closed(&self, state: &mut State) {
        info!(
            backend = %self.backend,
            consecutive_successes = state.consecutive_successes,
            "Circuit breaker closing"
        );

        state.circuit_state = CircuitState::Closed;
        state.opened_at = None;
        state.consecutive_failures = 0;
        state.consecutive_successes = 0;
        state.half_open_requests = 0;
        state.metrics.circuit_closed_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new("test-backend".to_string(), CircuitBreakerConfig::default());
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.can_proceed().await);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_threshold_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test-backend".to_string(), config);

        // Record failures
        for _ in 0..3 {
            assert!(cb.can_proceed().await);
            cb.record_failure().await;
        }

        // Circuit should be open now
        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.can_proceed().await);
    }

    #[tokio::test]
    async fn test_circuit_resets_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test-backend".to_string(), config);

        // Record some failures
        for _ in 0..2 {
            assert!(cb.can_proceed().await);
            cb.record_failure().await;
        }

        // Record a success
        assert!(cb.can_proceed().await);
        cb.record_success().await;

        // Circuit should still be closed
        assert_eq!(cb.state().await, CircuitState::Closed);

        // Record more failures to reach threshold
        for _ in 0..3 {
            assert!(cb.can_proceed().await);
            cb.record_failure().await;
        }

        // Now circuit should be open
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_half_open_allows_limited_requests() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            half_open_requests: 2,
            timeout_secs: 0, // Immediate transition to half-open
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test-backend".to_string(), config);

        // Open the circuit
        for _ in 0..2 {
            assert!(cb.can_proceed().await);
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait a bit and try again (circuit should transition to half-open)
        tokio::time::sleep(Duration::from_millis(10)).await;

        // First half-open request should be allowed
        assert!(cb.can_proceed().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Second half-open request should be allowed
        assert!(cb.can_proceed().await);

        // Third request should be rejected (max half-open requests = 2)
        assert!(!cb.can_proceed().await);
    }

    #[tokio::test]
    async fn test_half_open_closes_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            half_open_requests: 3,
            timeout_secs: 0,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test-backend".to_string(), config);

        // Open the circuit
        for _ in 0..2 {
            assert!(cb.can_proceed().await);
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait and transition to half-open
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(cb.can_proceed().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record successes
        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        assert!(cb.can_proceed().await);
        cb.record_success().await;

        // Circuit should close after reaching success threshold
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_reopens_on_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout_secs: 0,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test-backend".to_string(), config);

        // Open the circuit
        for _ in 0..2 {
            assert!(cb.can_proceed().await);
            cb.record_failure().await;
        }
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait and transition to half-open
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(cb.can_proceed().await);
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record a failure
        cb.record_failure().await;

        // Circuit should reopen
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_metrics_tracking() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test-backend".to_string(), config);

        // Make some requests
        assert!(cb.can_proceed().await);
        cb.record_success().await;

        assert!(cb.can_proceed().await);
        cb.record_failure().await;

        assert!(cb.can_proceed().await);
        cb.record_failure().await;

        // Circuit should be open
        assert_eq!(cb.state().await, CircuitState::Open);

        // Try to make a request (should be rejected)
        assert!(!cb.can_proceed().await);

        let metrics = cb.metrics().await;
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.successful_requests, 1);
        assert_eq!(metrics.failed_requests, 2);
        assert_eq!(metrics.rejected_requests, 1);
        assert_eq!(metrics.circuit_opened_count, 1);
    }
}

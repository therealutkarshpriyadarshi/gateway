use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, allowing probe requests
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "Closed"),
            CircuitState::Open => write!(f, "Open"),
            CircuitState::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Number of consecutive successes in half-open state before closing
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,

    /// Duration to wait in open state before transitioning to half-open
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Number of requests to allow in half-open state
    #[serde(default = "default_half_open_requests")]
    pub half_open_requests: u32,

    /// Timeout for individual requests in seconds
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
}

fn default_failure_threshold() -> u32 {
    5
}

fn default_success_threshold() -> u32 {
    2
}

fn default_timeout_secs() -> u64 {
    60
}

fn default_half_open_requests() -> u32 {
    3
}

fn default_request_timeout_secs() -> u64 {
    30
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            timeout_secs: default_timeout_secs(),
            half_open_requests: default_half_open_requests(),
            request_timeout_secs: default_request_timeout_secs(),
        }
    }
}

impl CircuitBreakerConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Initial backoff duration in milliseconds
    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,

    /// Maximum backoff duration in milliseconds
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,

    /// Backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_max_retries() -> u32 {
    3
}

fn default_initial_backoff_ms() -> u64 {
    100
}

fn default_max_backoff_ms() -> u64 {
    10000
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            initial_backoff_ms: default_initial_backoff_ms(),
            max_backoff_ms: default_max_backoff_ms(),
            backoff_multiplier: default_backoff_multiplier(),
        }
    }
}

impl RetryConfig {
    pub fn initial_backoff(&self) -> Duration {
        Duration::from_millis(self.initial_backoff_ms)
    }

    pub fn max_backoff(&self) -> Duration {
        Duration::from_millis(self.max_backoff_ms)
    }
}

/// Circuit breaker metrics
#[derive(Debug, Clone, Default)]
pub struct CircuitBreakerMetrics {
    /// Total number of requests
    pub total_requests: u64,
    /// Number of successful requests
    pub successful_requests: u64,
    /// Number of failed requests
    pub failed_requests: u64,
    /// Number of requests rejected (circuit open)
    pub rejected_requests: u64,
    /// Number of timeouts
    pub timeout_count: u64,
    /// Number of times circuit opened
    pub circuit_opened_count: u64,
    /// Number of times circuit closed
    pub circuit_closed_count: u64,
    /// Number of times circuit half-opened
    pub circuit_half_opened_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_state_display() {
        assert_eq!(CircuitState::Closed.to_string(), "Closed");
        assert_eq!(CircuitState::Open.to_string(), "Open");
        assert_eq!(CircuitState::HalfOpen.to_string(), "HalfOpen");
    }

    #[test]
    fn test_default_config() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.half_open_requests, 3);
        assert_eq!(config.request_timeout_secs, 30);
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_backoff_ms, 100);
        assert_eq!(config.max_backoff_ms, 10000);
        assert_eq!(config.backoff_multiplier, 2.0);
    }
}

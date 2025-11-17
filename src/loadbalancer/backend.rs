use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Backend server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Backend URL
    pub url: String,
    /// Weight for weighted load balancing (default: 1)
    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

/// Backend server state
#[derive(Debug, Clone)]
pub struct Backend {
    /// Backend configuration
    pub config: BackendConfig,
    /// Health status
    health: Arc<HealthStatus>,
    /// Active connections counter
    active_connections: Arc<AtomicUsize>,
}

/// Health status of a backend
#[derive(Debug)]
struct HealthStatus {
    /// Whether the backend is healthy
    is_healthy: AtomicBool,
    /// Consecutive successful health checks
    consecutive_successes: AtomicUsize,
    /// Consecutive failed health checks
    consecutive_failures: AtomicUsize,
    /// Last health check timestamp (Unix epoch seconds)
    last_check: AtomicU64,
    /// Total successful requests
    total_successes: AtomicU64,
    /// Total failed requests
    total_failures: AtomicU64,
}

impl Backend {
    /// Create a new backend from configuration
    pub fn new(config: BackendConfig) -> Self {
        Self {
            config,
            health: Arc::new(HealthStatus {
                is_healthy: AtomicBool::new(true), // Start as healthy
                consecutive_successes: AtomicUsize::new(0),
                consecutive_failures: AtomicUsize::new(0),
                last_check: AtomicU64::new(0),
                total_successes: AtomicU64::new(0),
                total_failures: AtomicU64::new(0),
            }),
            active_connections: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get backend URL
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Get backend weight
    pub fn weight(&self) -> u32 {
        self.config.weight
    }

    /// Check if backend is healthy
    pub fn is_healthy(&self) -> bool {
        self.health.is_healthy.load(Ordering::Relaxed)
    }

    /// Get active connections count
    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Increment active connections
    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connections
    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record a successful request
    pub fn record_success(&self) {
        self.health.total_successes.fetch_add(1, Ordering::Relaxed);
        self.health
            .consecutive_successes
            .fetch_add(1, Ordering::Relaxed);
        self.health.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        self.health.total_failures.fetch_add(1, Ordering::Relaxed);
        self.health
            .consecutive_failures
            .fetch_add(1, Ordering::Relaxed);
        self.health
            .consecutive_successes
            .store(0, Ordering::Relaxed);
    }

    /// Mark backend as healthy
    pub fn mark_healthy(&self) {
        self.health.is_healthy.store(true, Ordering::Relaxed);
    }

    /// Mark backend as unhealthy
    pub fn mark_unhealthy(&self) {
        self.health.is_healthy.store(false, Ordering::Relaxed);
    }

    /// Update health check based on consecutive failures
    pub fn update_health_from_passive_check(
        &self,
        unhealthy_threshold: usize,
        healthy_threshold: usize,
    ) {
        let consecutive_failures = self.health.consecutive_failures.load(Ordering::Relaxed);
        let consecutive_successes = self.health.consecutive_successes.load(Ordering::Relaxed);

        if consecutive_failures >= unhealthy_threshold {
            self.mark_unhealthy();
        } else if consecutive_successes >= healthy_threshold {
            self.mark_healthy();
        }
    }

    /// Record health check result
    pub fn record_health_check(
        &self,
        success: bool,
        unhealthy_threshold: usize,
        healthy_threshold: usize,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();

        self.health.last_check.store(now, Ordering::Relaxed);

        if success {
            let successes = self
                .health
                .consecutive_successes
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            self.health.consecutive_failures.store(0, Ordering::Relaxed);

            if successes >= healthy_threshold {
                self.mark_healthy();
            }
        } else {
            let failures = self
                .health
                .consecutive_failures
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            self.health
                .consecutive_successes
                .store(0, Ordering::Relaxed);

            if failures >= unhealthy_threshold {
                self.mark_unhealthy();
            }
        }
    }

    /// Get health statistics
    pub fn health_stats(&self) -> HealthStats {
        HealthStats {
            is_healthy: self.is_healthy(),
            consecutive_successes: self.health.consecutive_successes.load(Ordering::Relaxed),
            consecutive_failures: self.health.consecutive_failures.load(Ordering::Relaxed),
            total_successes: self.health.total_successes.load(Ordering::Relaxed),
            total_failures: self.health.total_failures.load(Ordering::Relaxed),
            active_connections: self.active_connections(),
        }
    }
}

/// Health statistics for a backend
#[derive(Debug, Clone, Serialize)]
pub struct HealthStats {
    pub is_healthy: bool,
    pub consecutive_successes: usize,
    pub consecutive_failures: usize,
    pub total_successes: u64,
    pub total_failures: u64,
    pub active_connections: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let config = BackendConfig {
            url: "http://localhost:3000".to_string(),
            weight: 1,
        };
        let backend = Backend::new(config);

        assert_eq!(backend.url(), "http://localhost:3000");
        assert_eq!(backend.weight(), 1);
        assert!(backend.is_healthy());
        assert_eq!(backend.active_connections(), 0);
    }

    #[test]
    fn test_connection_tracking() {
        let config = BackendConfig {
            url: "http://localhost:3000".to_string(),
            weight: 1,
        };
        let backend = Backend::new(config);

        assert_eq!(backend.active_connections(), 0);

        backend.increment_connections();
        assert_eq!(backend.active_connections(), 1);

        backend.increment_connections();
        assert_eq!(backend.active_connections(), 2);

        backend.decrement_connections();
        assert_eq!(backend.active_connections(), 1);
    }

    #[test]
    fn test_health_tracking() {
        let config = BackendConfig {
            url: "http://localhost:3000".to_string(),
            weight: 1,
        };
        let backend = Backend::new(config);

        assert!(backend.is_healthy());

        backend.mark_unhealthy();
        assert!(!backend.is_healthy());

        backend.mark_healthy();
        assert!(backend.is_healthy());
    }

    #[test]
    fn test_passive_health_check() {
        let config = BackendConfig {
            url: "http://localhost:3000".to_string(),
            weight: 1,
        };
        let backend = Backend::new(config);

        // Record some failures
        backend.record_failure();
        backend.record_failure();
        backend.record_failure();

        // Update health with threshold of 3 failures
        backend.update_health_from_passive_check(3, 2);
        assert!(!backend.is_healthy());

        // Record some successes
        backend.record_success();
        backend.record_success();

        // Update health with threshold of 2 successes
        backend.update_health_from_passive_check(3, 2);
        assert!(backend.is_healthy());
    }

    #[test]
    fn test_health_check_recording() {
        let config = BackendConfig {
            url: "http://localhost:3000".to_string(),
            weight: 1,
        };
        let backend = Backend::new(config);

        // Record failures
        backend.record_health_check(false, 3, 2);
        backend.record_health_check(false, 3, 2);
        backend.record_health_check(false, 3, 2);

        assert!(!backend.is_healthy());

        // Record successes
        backend.record_health_check(true, 3, 2);
        backend.record_health_check(true, 3, 2);

        assert!(backend.is_healthy());
    }

    #[test]
    fn test_health_stats() {
        let config = BackendConfig {
            url: "http://localhost:3000".to_string(),
            weight: 1,
        };
        let backend = Backend::new(config);

        backend.record_success();
        backend.record_success();
        backend.record_failure();
        backend.increment_connections();

        let stats = backend.health_stats();
        assert_eq!(stats.total_successes, 2);
        assert_eq!(stats.total_failures, 1);
        assert_eq!(stats.consecutive_failures, 1);
        assert_eq!(stats.active_connections, 1);
    }
}

use crate::loadbalancer::backend::Backend;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Enable health checks
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Interval between health checks in seconds
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Health check timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Number of consecutive failures before marking unhealthy
    #[serde(default = "default_unhealthy_threshold")]
    pub unhealthy_threshold: usize,
    /// Number of consecutive successes before marking healthy
    #[serde(default = "default_healthy_threshold")]
    pub healthy_threshold: usize,
    /// HTTP path to check (e.g., "/health")
    #[serde(default = "default_path")]
    pub path: String,
    /// Expected HTTP status code
    #[serde(default = "default_expected_status")]
    pub expected_status: u16,
    /// Enable passive health checks (based on request failures)
    #[serde(default = "default_enabled")]
    pub passive_enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_interval() -> u64 {
    30
}

fn default_timeout() -> u64 {
    5
}

fn default_unhealthy_threshold() -> usize {
    3
}

fn default_healthy_threshold() -> usize {
    2
}

fn default_path() -> String {
    "/health".to_string()
}

fn default_expected_status() -> u16 {
    200
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            interval_secs: default_interval(),
            timeout_secs: default_timeout(),
            unhealthy_threshold: default_unhealthy_threshold(),
            healthy_threshold: default_healthy_threshold(),
            path: default_path(),
            expected_status: default_expected_status(),
            passive_enabled: default_enabled(),
        }
    }
}

/// Health checker for monitoring backend health
pub struct HealthChecker {
    config: HealthCheckConfig,
    client: reqwest::Client,
}

impl std::fmt::Debug for HealthChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthChecker")
            .field("config", &self.config)
            .field("client", &"<reqwest::Client>")
            .finish()
    }
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(config: HealthCheckConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create health check client");

        Self { config, client }
    }

    /// Start active health checking for a set of backends
    pub fn start_active_checks(&self, backends: Vec<Arc<Backend>>) {
        if !self.config.enabled {
            info!("Active health checks disabled");
            return;
        }

        let config = self.config.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut check_interval = interval(Duration::from_secs(config.interval_secs));

            info!(
                interval_secs = config.interval_secs,
                path = %config.path,
                "Started active health checks"
            );

            loop {
                check_interval.tick().await;

                for backend in &backends {
                    let url = format!("{}{}", backend.url().trim_end_matches('/'), config.path);

                    debug!(url = %url, "Performing health check");

                    let result = client.get(&url).send().await;

                    let success = match result {
                        Ok(response) => {
                            let status = response.status();
                            let success = status.as_u16() == config.expected_status;

                            if success {
                                debug!(
                                    backend = %backend.url(),
                                    status = %status,
                                    "Health check passed"
                                );
                            } else {
                                warn!(
                                    backend = %backend.url(),
                                    status = %status,
                                    expected = config.expected_status,
                                    "Health check failed: unexpected status"
                                );
                            }

                            success
                        }
                        Err(e) => {
                            warn!(
                                backend = %backend.url(),
                                error = %e,
                                "Health check failed: request error"
                            );
                            false
                        }
                    };

                    // Record health check result
                    backend.record_health_check(
                        success,
                        config.unhealthy_threshold,
                        config.healthy_threshold,
                    );

                    // Log health status changes
                    if !backend.is_healthy() {
                        error!(
                            backend = %backend.url(),
                            "Backend marked unhealthy"
                        );
                    }
                }
            }
        });
    }

    /// Perform passive health check based on request result
    pub fn passive_check(&self, backend: &Backend, success: bool) {
        if !self.config.passive_enabled {
            return;
        }

        if success {
            backend.record_success();
        } else {
            backend.record_failure();
        }

        backend.update_health_from_passive_check(
            self.config.unhealthy_threshold,
            self.config.healthy_threshold,
        );
    }

    /// Get health check configuration
    pub fn config(&self) -> &HealthCheckConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loadbalancer::backend::BackendConfig;

    #[test]
    fn test_default_config() {
        let config = HealthCheckConfig::default();
        assert!(config.enabled);
        assert_eq!(config.interval_secs, 30);
        assert_eq!(config.timeout_secs, 5);
        assert_eq!(config.unhealthy_threshold, 3);
        assert_eq!(config.healthy_threshold, 2);
        assert_eq!(config.path, "/health");
        assert_eq!(config.expected_status, 200);
        assert!(config.passive_enabled);
    }

    #[test]
    fn test_health_checker_creation() {
        let config = HealthCheckConfig::default();
        let _checker = HealthChecker::new(config);
        // Just verify it doesn't panic
    }

    #[test]
    fn test_passive_check() {
        let config = HealthCheckConfig::default();
        let checker = HealthChecker::new(config);

        let backend = Arc::new(Backend::new(BackendConfig {
            url: "http://test:3000".to_string(),
            weight: 1,
        }));

        // Record some failures
        for _ in 0..3 {
            checker.passive_check(&backend, false);
        }

        assert!(!backend.is_healthy());

        // Record some successes
        for _ in 0..2 {
            checker.passive_check(&backend, true);
        }

        assert!(backend.is_healthy());
    }

    #[tokio::test]
    async fn test_active_checks_disabled() {
        let config = HealthCheckConfig {
            enabled: false,
            ..Default::default()
        };

        let checker = HealthChecker::new(config);

        let backends = vec![Arc::new(Backend::new(BackendConfig {
            url: "http://test:3000".to_string(),
            weight: 1,
        }))];

        checker.start_active_checks(backends);

        // Should not panic and should return immediately
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

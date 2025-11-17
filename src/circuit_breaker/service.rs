use super::breaker::CircuitBreaker;
use super::types::{CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::debug;

/// Circuit breaker service managing multiple backends
#[derive(Debug, Clone)]
pub struct CircuitBreakerService {
    /// Circuit breakers per backend
    breakers: Arc<DashMap<String, Arc<CircuitBreaker>>>,
    /// Default configuration
    config: CircuitBreakerConfig,
}

impl CircuitBreakerService {
    /// Create a new circuit breaker service
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Get or create a circuit breaker for a backend
    fn get_or_create_breaker(&self, backend: &str) -> Arc<CircuitBreaker> {
        self.breakers
            .entry(backend.to_string())
            .or_insert_with(|| {
                debug!(backend = backend, "Creating new circuit breaker");
                Arc::new(CircuitBreaker::new(backend.to_string(), self.config.clone()))
            })
            .clone()
    }

    /// Check if a request can proceed for a backend
    pub async fn can_proceed(&self, backend: &str) -> bool {
        let breaker = self.get_or_create_breaker(backend);
        breaker.can_proceed().await
    }

    /// Record a successful request for a backend
    pub async fn record_success(&self, backend: &str) {
        let breaker = self.get_or_create_breaker(backend);
        breaker.record_success().await;
    }

    /// Record a failed request for a backend
    pub async fn record_failure(&self, backend: &str) {
        let breaker = self.get_or_create_breaker(backend);
        breaker.record_failure().await;
    }

    /// Record a timeout for a backend
    pub async fn record_timeout(&self, backend: &str) {
        let breaker = self.get_or_create_breaker(backend);
        breaker.record_timeout().await;
    }

    /// Get the state of a circuit breaker for a backend
    pub async fn state(&self, backend: &str) -> CircuitState {
        if let Some(breaker) = self.breakers.get(backend) {
            breaker.state().await
        } else {
            CircuitState::Closed
        }
    }

    /// Get metrics for a backend
    pub async fn metrics(&self, backend: &str) -> Option<CircuitBreakerMetrics> {
        if let Some(breaker) = self.breakers.get(backend) {
            Some(breaker.metrics().await)
        } else {
            None
        }
    }

    /// Get all backend names with circuit breakers
    pub fn backends(&self) -> Vec<String> {
        self.breakers.iter().map(|e| e.key().clone()).collect()
    }

    /// Get metrics for all backends
    pub async fn all_metrics(&self) -> Vec<(String, CircuitBreakerMetrics, CircuitState)> {
        let mut results = Vec::new();
        for entry in self.breakers.iter() {
            let backend = entry.key().clone();
            let breaker = entry.value().clone();
            let metrics = breaker.metrics().await;
            let state = breaker.state().await;
            results.push((backend, metrics, state));
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_manages_multiple_backends() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let service = CircuitBreakerService::new(config);

        // Test backend1
        assert!(service.can_proceed("backend1").await);
        service.record_success("backend1").await;

        // Test backend2
        assert!(service.can_proceed("backend2").await);
        service.record_failure("backend2").await;
        assert!(service.can_proceed("backend2").await);
        service.record_failure("backend2").await;

        // backend1 should still be closed
        assert_eq!(service.state("backend1").await, CircuitState::Closed);

        // backend2 should be open
        assert_eq!(service.state("backend2").await, CircuitState::Open);

        // Check backends list
        let backends = service.backends();
        assert_eq!(backends.len(), 2);
        assert!(backends.contains(&"backend1".to_string()));
        assert!(backends.contains(&"backend2".to_string()));
    }

    #[tokio::test]
    async fn test_service_all_metrics() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let service = CircuitBreakerService::new(config);

        // Make some requests
        assert!(service.can_proceed("backend1").await);
        service.record_success("backend1").await;

        assert!(service.can_proceed("backend2").await);
        service.record_failure("backend2").await;

        let all_metrics = service.all_metrics().await;
        assert_eq!(all_metrics.len(), 2);

        // Find backend1 metrics
        let backend1_metrics = all_metrics
            .iter()
            .find(|(name, _, _)| name == "backend1")
            .unwrap();
        assert_eq!(backend1_metrics.1.successful_requests, 1);

        // Find backend2 metrics
        let backend2_metrics = all_metrics
            .iter()
            .find(|(name, _, _)| name == "backend2")
            .unwrap();
        assert_eq!(backend2_metrics.1.failed_requests, 1);
    }

    #[tokio::test]
    async fn test_service_nonexistent_backend() {
        let config = CircuitBreakerConfig::default();
        let service = CircuitBreakerService::new(config);

        // Query non-existent backend
        assert_eq!(service.state("nonexistent").await, CircuitState::Closed);
        assert!(service.metrics("nonexistent").await.is_none());
    }
}

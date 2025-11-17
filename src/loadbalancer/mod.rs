pub mod backend;
pub mod strategies;

use backend::{Backend, BackendConfig};
use std::net::IpAddr;
use std::sync::Arc;
use strategies::LoadBalancingStrategy;

/// Load balancer for distributing requests across multiple backends
#[derive(Debug, Clone)]
pub struct LoadBalancer {
    /// Available backends
    backends: Vec<Arc<Backend>>,
    /// Load balancing strategy
    strategy: LoadBalancingStrategy,
}

impl LoadBalancer {
    /// Create a new load balancer
    pub fn new(backend_configs: Vec<BackendConfig>, strategy: LoadBalancingStrategy) -> Self {
        let backends = backend_configs
            .into_iter()
            .map(|config| Arc::new(Backend::new(config)))
            .collect();

        Self { backends, strategy }
    }

    /// Select a backend for the request
    pub fn select_backend(&self, client_ip: Option<IpAddr>) -> Option<Arc<Backend>> {
        self.strategy.select(&self.backends, client_ip).cloned()
    }

    /// Get all backends
    pub fn backends(&self) -> &[Arc<Backend>] {
        &self.backends
    }

    /// Get healthy backend count
    pub fn healthy_count(&self) -> usize {
        self.backends.iter().filter(|b| b.is_healthy()).count()
    }

    /// Get total backend count
    pub fn total_count(&self) -> usize {
        self.backends.len()
    }

    /// Check if any backend is healthy
    pub fn has_healthy_backend(&self) -> bool {
        self.backends.iter().any(|b| b.is_healthy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strategies::{RoundRobinStrategy, WeightedStrategy};

    fn create_test_configs(count: usize) -> Vec<BackendConfig> {
        (0..count)
            .map(|i| BackendConfig {
                url: format!("http://backend-{}", i),
                weight: 1,
            })
            .collect()
    }

    #[test]
    fn test_load_balancer_creation() {
        let configs = create_test_configs(3);
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let lb = LoadBalancer::new(configs, strategy);

        assert_eq!(lb.total_count(), 3);
        assert_eq!(lb.healthy_count(), 3);
        assert!(lb.has_healthy_backend());
    }

    #[test]
    fn test_select_backend() {
        let configs = create_test_configs(3);
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let lb = LoadBalancer::new(configs, strategy);

        let backend = lb.select_backend(None);
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().url(), "http://backend-0");
    }

    #[test]
    fn test_round_robin_distribution() {
        let configs = create_test_configs(3);
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let lb = LoadBalancer::new(configs, strategy);

        let b1 = lb.select_backend(None).unwrap();
        let b2 = lb.select_backend(None).unwrap();
        let b3 = lb.select_backend(None).unwrap();
        let b4 = lb.select_backend(None).unwrap();

        assert_eq!(b1.url(), "http://backend-0");
        assert_eq!(b2.url(), "http://backend-1");
        assert_eq!(b3.url(), "http://backend-2");
        assert_eq!(b4.url(), "http://backend-0");
    }

    #[test]
    fn test_weighted_distribution() {
        let configs = vec![
            BackendConfig {
                url: "http://backend-0".to_string(),
                weight: 1,
            },
            BackendConfig {
                url: "http://backend-1".to_string(),
                weight: 2,
            },
        ];
        let strategy = LoadBalancingStrategy::Weighted(WeightedStrategy::new());
        let lb = LoadBalancer::new(configs, strategy);

        let mut counts = std::collections::HashMap::new();
        for _ in 0..30 {
            let backend = lb.select_backend(None).unwrap();
            *counts.entry(backend.url().to_string()).or_insert(0) += 1;
        }

        // Backend-0 (weight 1) should get 10 requests
        // Backend-1 (weight 2) should get 20 requests
        assert_eq!(counts.get("http://backend-0"), Some(&10));
        assert_eq!(counts.get("http://backend-1"), Some(&20));
    }

    #[test]
    fn test_unhealthy_backends() {
        let configs = create_test_configs(3);
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let lb = LoadBalancer::new(configs, strategy);

        // Mark one backend as unhealthy
        lb.backends()[1].mark_unhealthy();

        assert_eq!(lb.healthy_count(), 2);
        assert!(lb.has_healthy_backend());

        // Should skip unhealthy backend
        for _ in 0..10 {
            let backend = lb.select_backend(None).unwrap();
            assert_ne!(backend.url(), "http://backend-1");
        }
    }

    #[test]
    fn test_all_unhealthy() {
        let configs = create_test_configs(2);
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let lb = LoadBalancer::new(configs, strategy);

        // Mark all backends as unhealthy
        for backend in lb.backends() {
            backend.mark_unhealthy();
        }

        assert_eq!(lb.healthy_count(), 0);
        assert!(!lb.has_healthy_backend());

        // Should return None
        let backend = lb.select_backend(None);
        assert!(backend.is_none());
    }

    #[test]
    fn test_least_connections() {
        let configs = create_test_configs(3);
        let strategy = LoadBalancingStrategy::LeastConnections;
        let lb = LoadBalancer::new(configs, strategy);

        // Add connections to first backend
        lb.backends()[0].increment_connections();
        lb.backends()[0].increment_connections();

        // Should select backend with fewest connections
        let backend = lb.select_backend(None).unwrap();
        assert!(backend.url() != "http://backend-0");
    }
}

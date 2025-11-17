use super::backend::Backend;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Load balancing strategy
#[derive(Debug, Clone)]
pub enum LoadBalancingStrategy {
    /// Round-robin: distribute requests evenly across backends
    RoundRobin(RoundRobinStrategy),
    /// Least connections: send to backend with fewest active connections
    LeastConnections,
    /// Weighted round-robin: distribute based on backend weights
    Weighted(WeightedStrategy),
    /// IP hash: consistent hashing based on client IP
    IpHash,
}

impl LoadBalancingStrategy {
    /// Select a backend from the available backends
    pub fn select<'a>(
        &self,
        backends: &'a [Arc<Backend>],
        client_ip: Option<IpAddr>,
    ) -> Option<&'a Arc<Backend>> {
        // Filter out unhealthy backends
        let healthy_backends: Vec<&Arc<Backend>> =
            backends.iter().filter(|b| b.is_healthy()).collect();

        if healthy_backends.is_empty() {
            return None;
        }

        match self {
            LoadBalancingStrategy::RoundRobin(strategy) => strategy.select(&healthy_backends),
            LoadBalancingStrategy::LeastConnections => {
                Self::select_least_connections(&healthy_backends)
            }
            LoadBalancingStrategy::Weighted(strategy) => strategy.select(&healthy_backends),
            LoadBalancingStrategy::IpHash => Self::select_ip_hash(&healthy_backends, client_ip),
        }
    }

    /// Select backend with least active connections
    fn select_least_connections<'a>(backends: &[&'a Arc<Backend>]) -> Option<&'a Arc<Backend>> {
        backends
            .iter()
            .min_by_key(|b| b.active_connections())
            .copied()
    }

    /// Select backend using IP hash
    fn select_ip_hash<'a>(
        backends: &[&'a Arc<Backend>],
        client_ip: Option<IpAddr>,
    ) -> Option<&'a Arc<Backend>> {
        let client_ip = client_ip?;

        // Simple hash of IP address
        let hash = match client_ip {
            IpAddr::V4(ip) => {
                let octets = ip.octets();
                (octets[0] as usize) << 24
                    | (octets[1] as usize) << 16
                    | (octets[2] as usize) << 8
                    | (octets[3] as usize)
            }
            IpAddr::V6(ip) => {
                let segments = ip.segments();
                // Use first 4 segments for hash
                (segments[0] as usize) << 48
                    | (segments[1] as usize) << 32
                    | (segments[2] as usize) << 16
                    | (segments[3] as usize)
            }
        };

        let index = hash % backends.len();
        backends.get(index).copied()
    }
}

/// Round-robin strategy state
#[derive(Debug, Clone)]
pub struct RoundRobinStrategy {
    counter: Arc<AtomicUsize>,
}

impl RoundRobinStrategy {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn select<'a>(&self, backends: &[&'a Arc<Backend>]) -> Option<&'a Arc<Backend>> {
        if backends.is_empty() {
            return None;
        }

        let index = self.counter.fetch_add(1, Ordering::Relaxed) % backends.len();
        backends.get(index).copied()
    }
}

impl Default for RoundRobinStrategy {
    fn default() -> Self {
        Self::new()
    }
}

/// Weighted round-robin strategy state
#[derive(Debug, Clone)]
pub struct WeightedStrategy {
    counter: Arc<AtomicUsize>,
}

impl WeightedStrategy {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn select<'a>(&self, backends: &[&'a Arc<Backend>]) -> Option<&'a Arc<Backend>> {
        if backends.is_empty() {
            return None;
        }

        // Calculate total weight
        let total_weight: u32 = backends.iter().map(|b| b.weight()).sum();

        if total_weight == 0 {
            // Fallback to round-robin if all weights are 0
            let index = self.counter.fetch_add(1, Ordering::Relaxed) % backends.len();
            return backends.get(index).copied();
        }

        // Get next counter value
        let counter = self.counter.fetch_add(1, Ordering::Relaxed);
        let position = (counter as u32) % total_weight;

        // Find backend based on weighted position
        let mut cumulative_weight = 0u32;
        for backend in backends {
            cumulative_weight += backend.weight();
            if position < cumulative_weight {
                return Some(backend);
            }
        }

        // Fallback to first backend (shouldn't reach here)
        backends.first().copied()
    }
}

impl Default for WeightedStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loadbalancer::backend::BackendConfig;
    use std::collections::HashMap;

    fn create_test_backends(count: usize) -> Vec<Arc<Backend>> {
        (0..count)
            .map(|i| {
                Arc::new(Backend::new(BackendConfig {
                    url: format!("http://backend-{}", i),
                    weight: 1,
                }))
            })
            .collect()
    }

    fn create_weighted_backends() -> Vec<Arc<Backend>> {
        vec![
            Arc::new(Backend::new(BackendConfig {
                url: "http://backend-0".to_string(),
                weight: 1,
            })),
            Arc::new(Backend::new(BackendConfig {
                url: "http://backend-1".to_string(),
                weight: 2,
            })),
            Arc::new(Backend::new(BackendConfig {
                url: "http://backend-2".to_string(),
                weight: 3,
            })),
        ]
    }

    #[test]
    fn test_round_robin() {
        let backends = create_test_backends(3);
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());

        // First request should go to backend-0
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-0");

        // Second request should go to backend-1
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-1");

        // Third request should go to backend-2
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-2");

        // Fourth request should wrap around to backend-0
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-0");
    }

    #[test]
    fn test_round_robin_with_unhealthy() {
        let backends = create_test_backends(3);
        backends[1].mark_unhealthy();

        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());

        // Should skip unhealthy backend
        let selected1 = strategy.select(&backends, None).unwrap();
        let selected2 = strategy.select(&backends, None).unwrap();
        let selected3 = strategy.select(&backends, None).unwrap();

        // Should only get backend-0 and backend-2
        assert!(selected1.url() != "http://backend-1");
        assert!(selected2.url() != "http://backend-1");
        assert!(selected3.url() != "http://backend-1");
    }

    #[test]
    fn test_least_connections() {
        let backends = create_test_backends(3);
        let strategy = LoadBalancingStrategy::LeastConnections;

        // All have 0 connections, should return first
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-0");

        // Add connections to backends
        backends[0].increment_connections();
        backends[0].increment_connections();
        backends[1].increment_connections();

        // Should select backend-2 (0 connections)
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-2");

        // Add connection to backend-2
        backends[2].increment_connections();

        // Should select backend-1 (1 connection)
        let selected = strategy.select(&backends, None).unwrap();
        assert_eq!(selected.url(), "http://backend-1");
    }

    #[test]
    fn test_weighted_strategy() {
        let backends = create_weighted_backends();
        let strategy = LoadBalancingStrategy::Weighted(WeightedStrategy::new());

        // Collect selections to verify distribution
        let mut selections = HashMap::new();
        for _ in 0..60 {
            let selected = strategy.select(&backends, None).unwrap();
            *selections.entry(selected.url().to_string()).or_insert(0) += 1;
        }

        // Backend-0 (weight 1) should get ~10 requests (1/6)
        // Backend-1 (weight 2) should get ~20 requests (2/6)
        // Backend-2 (weight 3) should get ~30 requests (3/6)
        let count0 = selections.get("http://backend-0").unwrap_or(&0);
        let count1 = selections.get("http://backend-1").unwrap_or(&0);
        let count2 = selections.get("http://backend-2").unwrap_or(&0);

        assert_eq!(*count0, 10);
        assert_eq!(*count1, 20);
        assert_eq!(*count2, 30);
    }

    #[test]
    fn test_ip_hash() {
        let backends = create_test_backends(3);
        let strategy = LoadBalancingStrategy::IpHash;

        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        // Same IP should always get same backend
        let selected1 = strategy.select(&backends, Some(ip1)).unwrap();
        let selected2 = strategy.select(&backends, Some(ip1)).unwrap();
        assert_eq!(selected1.url(), selected2.url());

        // Different IPs might get different backends
        let selected3 = strategy.select(&backends, Some(ip2)).unwrap();
        // We can't guarantee they're different, but we can verify it's one of the backends
        assert!(
            selected3.url() == "http://backend-0"
                || selected3.url() == "http://backend-1"
                || selected3.url() == "http://backend-2"
        );
    }

    #[test]
    fn test_ip_hash_ipv6() {
        let backends = create_test_backends(3);
        let strategy = LoadBalancingStrategy::IpHash;

        let ip1: IpAddr = "2001:db8::1".parse().unwrap();
        let ip2: IpAddr = "2001:db8::2".parse().unwrap();

        // Same IPv6 should always get same backend
        let selected1 = strategy.select(&backends, Some(ip1)).unwrap();
        let selected2 = strategy.select(&backends, Some(ip1)).unwrap();
        assert_eq!(selected1.url(), selected2.url());

        // Different IPv6s
        let selected3 = strategy.select(&backends, Some(ip2)).unwrap();
        assert!(
            selected3.url() == "http://backend-0"
                || selected3.url() == "http://backend-1"
                || selected3.url() == "http://backend-2"
        );
    }

    #[test]
    fn test_no_healthy_backends() {
        let backends = create_test_backends(2);
        backends[0].mark_unhealthy();
        backends[1].mark_unhealthy();

        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let selected = strategy.select(&backends, None);
        assert!(selected.is_none());
    }

    #[test]
    fn test_empty_backends() {
        let backends: Vec<Arc<Backend>> = vec![];
        let strategy = LoadBalancingStrategy::RoundRobin(RoundRobinStrategy::new());
        let selected = strategy.select(&backends, None);
        assert!(selected.is_none());
    }
}

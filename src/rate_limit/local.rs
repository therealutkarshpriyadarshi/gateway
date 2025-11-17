use super::types::{RateLimitConfig, RateLimitKey, RateLimitResult};
use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Local (in-memory) rate limiter using token bucket algorithm
pub struct LocalRateLimiter {
    /// Map of rate limiters per key
    #[allow(clippy::type_complexity)]
    limiters: Arc<DashMap<String, Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>,
    /// Default configuration
    config: RateLimitConfig,
}

impl LocalRateLimiter {
    /// Create a new local rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            limiters: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Check if a request is allowed
    pub async fn check_rate_limit(&self, key: &RateLimitKey) -> RateLimitResult {
        let redis_key = key.to_redis_key();

        // Get or create rate limiter for this key
        let limiter = self
            .limiters
            .entry(redis_key.clone())
            .or_insert_with(|| {
                debug!("Creating new rate limiter for key: {}", redis_key);
                Arc::new(self.create_limiter())
            })
            .clone();

        // Check the rate limit
        match limiter.check() {
            Ok(_) => {
                // Request allowed - we can't get exact remaining count from governor easily
                // so we'll report an estimate based on the config
                debug!("Rate limit check passed for key: {}", redis_key);

                RateLimitResult::allowed(
                    (self.config.requests / 2) as i64, // Conservative estimate
                    self.config.requests,
                    self.config.window_secs,
                )
            }
            Err(_) => {
                // Request denied - use the window duration as retry_after
                warn!("Rate limit exceeded for key: {}", redis_key);

                RateLimitResult::denied(self.config.requests, self.config.window_secs)
            }
        }
    }

    /// Create a new governor rate limiter
    fn create_limiter(&self) -> GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock> {
        let quota = if let Some(burst) = self.config.burst {
            // Create quota with custom burst
            Quota::with_period(Duration::from_secs(self.config.window_secs))
                .unwrap()
                .allow_burst(NonZeroU32::new(burst).unwrap())
        } else {
            // Create quota with default burst (same as limit)
            Quota::per_second(NonZeroU32::new(self.config.requests).unwrap())
                .allow_burst(NonZeroU32::new(self.config.requests).unwrap())
        };

        GovernorRateLimiter::direct(quota)
    }

    /// Get the number of active limiters (for testing/monitoring)
    pub fn active_limiters(&self) -> usize {
        self.limiters.len()
    }

    /// Clear all limiters (for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        self.limiters.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::types::RateLimitDimension;

    #[tokio::test]
    async fn test_local_rate_limiter_allows_within_limit() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 10,
            window_secs: 1,
            burst: None,
        };

        let limiter = LocalRateLimiter::new(config);
        let key = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.1".to_string());

        // First 10 requests should be allowed
        for i in 0..10 {
            let result = limiter.check_rate_limit(&key).await;
            assert!(
                result.allowed,
                "Request {} should be allowed (remaining: {})",
                i, result.remaining
            );
        }
    }

    #[tokio::test]
    async fn test_local_rate_limiter_denies_over_limit() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 5,
            window_secs: 60,
            burst: None,
        };

        let limiter = LocalRateLimiter::new(config);
        let key = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.2".to_string());

        // Use up the quota
        for _ in 0..5 {
            let result = limiter.check_rate_limit(&key).await;
            assert!(result.allowed);
        }

        // Next request should be denied
        let result = limiter.check_rate_limit(&key).await;
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
        assert!(result.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_local_rate_limiter_different_keys() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 2,
            window_secs: 60,
            burst: None,
        };

        let limiter = LocalRateLimiter::new(config);
        let key1 = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.1".to_string());
        let key2 = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.2".to_string());

        // Use up quota for key1
        for _ in 0..2 {
            let result = limiter.check_rate_limit(&key1).await;
            assert!(result.allowed);
        }

        // key1 should be denied
        let result = limiter.check_rate_limit(&key1).await;
        assert!(!result.allowed);

        // key2 should still be allowed
        let result = limiter.check_rate_limit(&key2).await;
        assert!(result.allowed);

        assert_eq!(limiter.active_limiters(), 2);
    }

    #[tokio::test]
    async fn test_local_rate_limiter_replenishment() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::User,
            requests: 2,
            window_secs: 1,
            burst: None,
        };

        let limiter = LocalRateLimiter::new(config);
        let key = RateLimitKey::new(RateLimitDimension::User, "user123".to_string());

        // Use up quota
        for _ in 0..2 {
            let result = limiter.check_rate_limit(&key).await;
            assert!(result.allowed);
        }

        // Should be denied
        let result = limiter.check_rate_limit(&key).await;
        assert!(!result.allowed);

        // Wait for replenishment
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should be allowed again
        let result = limiter.check_rate_limit(&key).await;
        assert!(result.allowed);
    }
}

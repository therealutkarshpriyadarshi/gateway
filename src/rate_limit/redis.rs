use super::lua_scripts::{FIXED_WINDOW_SCRIPT, SLIDING_WINDOW_SCRIPT, TOKEN_BUCKET_SCRIPT};
use super::types::{RateLimitConfig, RateLimitKey, RateLimitResult};
use redis::{aio::ConnectionManager, Script};
use std::time::SystemTime;
use tracing::{debug, error, warn};

/// Redis-backed distributed rate limiter
pub struct RedisRateLimiter {
    /// Redis connection manager
    connection: ConnectionManager,
    /// Rate limit configuration
    config: RateLimitConfig,
    /// Algorithm to use
    algorithm: RateLimitAlgorithm,
}

/// Rate limiting algorithm
#[derive(Debug, Clone)]
pub enum RateLimitAlgorithm {
    /// Token bucket algorithm (smooth rate limiting)
    TokenBucket,
    /// Sliding window counter (more accurate)
    SlidingWindow,
    /// Fixed window (simpler, less accurate)
    FixedWindow,
}

impl RedisRateLimiter {
    /// Create a new Redis rate limiter
    pub async fn new(
        redis_url: &str,
        config: RateLimitConfig,
        algorithm: RateLimitAlgorithm,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = ConnectionManager::new(client).await?;

        Ok(Self {
            connection,
            config,
            algorithm,
        })
    }

    /// Check if a request is allowed
    pub async fn check_rate_limit(&mut self, key: &RateLimitKey) -> RateLimitResult {
        let redis_key = key.to_redis_key();

        match self.algorithm {
            RateLimitAlgorithm::TokenBucket => self.check_token_bucket(&redis_key).await,
            RateLimitAlgorithm::SlidingWindow => self.check_sliding_window(&redis_key).await,
            RateLimitAlgorithm::FixedWindow => self.check_fixed_window(&redis_key).await,
        }
    }

    /// Check rate limit using token bucket algorithm
    async fn check_token_bucket(&mut self, key: &str) -> RateLimitResult {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let refill_rate = self.config.requests as f64 / self.config.window_secs as f64;

        let script = Script::new(TOKEN_BUCKET_SCRIPT);

        match script
            .key(key)
            .arg(self.config.requests)
            .arg(refill_rate)
            .arg(now)
            .arg(self.config.window_secs)
            .invoke_async::<_, Vec<i64>>(&mut self.connection)
            .await
        {
            Ok(result) => {
                let allowed = result[0] == 1;
                let remaining = result[1];
                let reset_after = result[2] as u64;

                debug!(
                    "Token bucket check for key {}: allowed={}, remaining={}, reset_after={}",
                    key, allowed, remaining, reset_after
                );

                if allowed {
                    RateLimitResult::allowed(remaining, self.config.requests, reset_after)
                } else {
                    warn!("Rate limit exceeded for key: {} (token bucket)", key);
                    RateLimitResult::denied(self.config.requests, reset_after)
                }
            }
            Err(e) => {
                error!("Redis error during rate limit check: {}", e);
                // On error, deny the request for safety
                RateLimitResult::denied(self.config.requests, self.config.window_secs)
            }
        }
    }

    /// Check rate limit using sliding window algorithm
    async fn check_sliding_window(&mut self, key: &str) -> RateLimitResult {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let script = Script::new(SLIDING_WINDOW_SCRIPT);

        match script
            .key(key)
            .arg(self.config.requests)
            .arg(self.config.window_secs)
            .arg(now)
            .invoke_async::<_, Vec<i64>>(&mut self.connection)
            .await
        {
            Ok(result) => {
                let allowed = result[0] == 1;
                let remaining = result[1];
                let reset_after = result[2] as u64;

                debug!(
                    "Sliding window check for key {}: allowed={}, remaining={}, reset_after={}",
                    key, allowed, remaining, reset_after
                );

                if allowed {
                    RateLimitResult::allowed(remaining, self.config.requests, reset_after)
                } else {
                    warn!("Rate limit exceeded for key: {} (sliding window)", key);
                    RateLimitResult::denied(self.config.requests, reset_after)
                }
            }
            Err(e) => {
                error!("Redis error during rate limit check: {}", e);
                RateLimitResult::denied(self.config.requests, self.config.window_secs)
            }
        }
    }

    /// Check rate limit using fixed window algorithm
    async fn check_fixed_window(&mut self, key: &str) -> RateLimitResult {
        let script = Script::new(FIXED_WINDOW_SCRIPT);

        match script
            .key(key)
            .arg(self.config.requests)
            .arg(self.config.window_secs)
            .invoke_async::<_, Vec<i64>>(&mut self.connection)
            .await
        {
            Ok(result) => {
                let allowed = result[0] == 1;
                let remaining = result[1];
                let reset_after = result[2] as u64;

                debug!(
                    "Fixed window check for key {}: allowed={}, remaining={}, reset_after={}",
                    key, allowed, remaining, reset_after
                );

                if allowed {
                    RateLimitResult::allowed(remaining, self.config.requests, reset_after)
                } else {
                    warn!("Rate limit exceeded for key: {} (fixed window)", key);
                    RateLimitResult::denied(self.config.requests, reset_after)
                }
            }
            Err(e) => {
                error!("Redis error during rate limit check: {}", e);
                RateLimitResult::denied(self.config.requests, self.config.window_secs)
            }
        }
    }

    /// Test Redis connection
    pub async fn ping(&mut self) -> Result<(), redis::RedisError> {
        redis::cmd("PING").query_async(&mut self.connection).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::types::RateLimitDimension;

    // Note: These tests require a running Redis instance
    // They are ignored by default. Run with: cargo test -- --ignored

    async fn create_test_limiter(algorithm: RateLimitAlgorithm) -> Option<RedisRateLimiter> {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 10,
            window_secs: 60,
            burst: None,
        };

        RedisRateLimiter::new("redis://127.0.0.1:6379", config, algorithm)
            .await
            .ok()
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_token_bucket() {
        let mut limiter = create_test_limiter(RateLimitAlgorithm::TokenBucket)
            .await
            .expect("Failed to connect to Redis");

        let key = RateLimitKey::new(
            RateLimitDimension::Ip,
            format!("test-tb-{}", rand::random::<u32>()),
        );

        // First requests should be allowed
        for _ in 0..10 {
            let result = limiter.check_rate_limit(&key).await;
            assert!(result.allowed);
        }

        // 11th request should be denied
        let result = limiter.check_rate_limit(&key).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_sliding_window() {
        let mut limiter = create_test_limiter(RateLimitAlgorithm::SlidingWindow)
            .await
            .expect("Failed to connect to Redis");

        let key = RateLimitKey::new(
            RateLimitDimension::User,
            format!("test-sw-{}", rand::random::<u32>()),
        );

        // First requests should be allowed
        for _ in 0..10 {
            let result = limiter.check_rate_limit(&key).await;
            assert!(result.allowed);
        }

        // 11th request should be denied
        let result = limiter.check_rate_limit(&key).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_fixed_window() {
        let mut limiter = create_test_limiter(RateLimitAlgorithm::FixedWindow)
            .await
            .expect("Failed to connect to Redis");

        let key = RateLimitKey::new(
            RateLimitDimension::ApiKey,
            format!("test-fw-{}", rand::random::<u32>()),
        );

        // First requests should be allowed
        for i in 0..10 {
            let result = limiter.check_rate_limit(&key).await;
            assert!(result.allowed, "Request {} should be allowed", i);
        }

        // 11th request should be denied
        let result = limiter.check_rate_limit(&key).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_connection() {
        let mut limiter = create_test_limiter(RateLimitAlgorithm::TokenBucket)
            .await
            .expect("Failed to connect to Redis");

        assert!(limiter.ping().await.is_ok());
    }
}

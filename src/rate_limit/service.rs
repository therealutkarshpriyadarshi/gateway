use super::local::LocalRateLimiter;
use super::redis::{RateLimitAlgorithm, RedisRateLimiter};
use super::types::{RateLimitConfig, RateLimitKey, RateLimitResult};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Rate limiter service that handles both local and distributed rate limiting
pub struct RateLimiterService {
    /// Local (in-memory) rate limiter
    local: Arc<LocalRateLimiter>,
    /// Redis-backed distributed rate limiter (optional)
    redis: Option<Arc<Mutex<RedisRateLimiter>>>,
    /// Whether to use Redis as primary
    use_redis_primary: bool,
}

impl RateLimiterService {
    /// Create a new rate limiter service with local-only rate limiting
    pub fn local_only(config: RateLimitConfig) -> Self {
        info!("Initializing local-only rate limiter");
        Self {
            local: Arc::new(LocalRateLimiter::new(config)),
            redis: None,
            use_redis_primary: false,
        }
    }

    /// Create a new rate limiter service with Redis backend and local fallback
    pub async fn with_redis(
        config: RateLimitConfig,
        redis_url: &str,
        algorithm: RateLimitAlgorithm,
    ) -> Result<Self, redis::RedisError> {
        info!("Initializing rate limiter with Redis backend");

        let mut redis_limiter = RedisRateLimiter::new(redis_url, config.clone(), algorithm).await?;

        // Test Redis connection
        match redis_limiter.ping().await {
            Ok(_) => {
                info!("Redis connection successful, using Redis as primary rate limiter");
                Ok(Self {
                    local: Arc::new(LocalRateLimiter::new(config)),
                    redis: Some(Arc::new(Mutex::new(redis_limiter))),
                    use_redis_primary: true,
                })
            }
            Err(e) => {
                warn!(
                    "Redis ping failed: {}, falling back to local rate limiter",
                    e
                );
                Ok(Self {
                    local: Arc::new(LocalRateLimiter::new(config)),
                    redis: Some(Arc::new(Mutex::new(redis_limiter))),
                    use_redis_primary: false,
                })
            }
        }
    }

    /// Check if a request is allowed based on rate limiting
    pub async fn check_rate_limit(&self, key: &RateLimitKey) -> RateLimitResult {
        if self.use_redis_primary {
            if let Some(redis) = &self.redis {
                match self.check_with_fallback(redis, key).await {
                    Some(result) => result,
                    None => {
                        // Redis failed, use local
                        warn!("Redis rate limit check failed, using local fallback");
                        self.local.check_rate_limit(key).await
                    }
                }
            } else {
                // No Redis configured, use local
                self.local.check_rate_limit(key).await
            }
        } else {
            // Use local rate limiter
            self.local.check_rate_limit(key).await
        }
    }

    /// Check rate limit with Redis and handle errors
    async fn check_with_fallback(
        &self,
        redis: &Arc<Mutex<RedisRateLimiter>>,
        key: &RateLimitKey,
    ) -> Option<RateLimitResult> {
        let mut redis_guard = redis.lock().await;
        let result = redis_guard.check_rate_limit(key).await;

        // Check if the result indicates a Redis error
        // (we treat deny as a valid result, not an error)
        if result.allowed || result.remaining == 0 {
            Some(result)
        } else {
            // This might be an error condition
            debug!("Redis returned unexpected result, falling back to local");
            None
        }
    }

    /// Check if Redis is available
    pub fn is_redis_available(&self) -> bool {
        self.redis.is_some() && self.use_redis_primary
    }

    /// Get the local rate limiter (for testing)
    #[cfg(test)]
    pub fn local(&self) -> &Arc<LocalRateLimiter> {
        &self.local
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::types::RateLimitDimension;

    #[tokio::test]
    async fn test_local_only_service() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 10,
            window_secs: 60,
            burst: None,
        };

        let service = RateLimiterService::local_only(config);
        assert!(!service.is_redis_available());

        let key = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.1".to_string());

        // Should allow requests up to the limit
        for _ in 0..10 {
            let result = service.check_rate_limit(&key).await;
            assert!(result.allowed);
        }

        // Should deny the 11th request
        let result = service.check_rate_limit(&key).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_service_different_dimensions() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 5,
            window_secs: 60,
            burst: None,
        };

        let service = RateLimiterService::local_only(config);

        let ip_key = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.1".to_string());
        let user_key = RateLimitKey::new(RateLimitDimension::User, "user123".to_string());

        // Use up IP limit
        for _ in 0..5 {
            let result = service.check_rate_limit(&ip_key).await;
            assert!(result.allowed);
        }

        // IP should be denied
        let result = service.check_rate_limit(&ip_key).await;
        assert!(!result.allowed);

        // User should still be allowed (different dimension)
        let result = service.check_rate_limit(&user_key).await;
        assert!(result.allowed);
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_redis_service() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 10,
            window_secs: 60,
            burst: None,
        };

        let service = RateLimiterService::with_redis(
            config,
            "redis://127.0.0.1:6379",
            RateLimitAlgorithm::SlidingWindow,
        )
        .await
        .expect("Failed to create Redis service");

        assert!(service.is_redis_available());

        let key = RateLimitKey::new(
            RateLimitDimension::Ip,
            format!("test-service-{}", rand::random::<u32>()),
        );

        // Should allow requests up to the limit
        for _ in 0..10 {
            let result = service.check_rate_limit(&key).await;
            assert!(result.allowed);
        }

        // Should deny the 11th request
        let result = service.check_rate_limit(&key).await;
        assert!(!result.allowed);
    }
}

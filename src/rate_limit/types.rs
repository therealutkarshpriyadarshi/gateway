use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Rate limit dimension - what to rate limit by
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitDimension {
    /// Rate limit by IP address
    Ip,
    /// Rate limit by user ID from JWT claims
    User,
    /// Rate limit by API key
    ApiKey,
    /// Rate limit by route path
    Route,
}

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Dimension to rate limit by
    pub dimension: RateLimitDimension,
    /// Maximum number of requests allowed
    pub requests: u32,
    /// Time window for the limit (in seconds)
    pub window_secs: u64,
    /// Burst size (if different from requests)
    #[serde(default)]
    pub burst: Option<u32>,
}

impl RateLimitConfig {
    /// Get the window as a Duration
    pub fn window(&self) -> Duration {
        Duration::from_secs(self.window_secs)
    }

    /// Get burst size (defaults to requests if not specified)
    pub fn burst_size(&self) -> u32 {
        self.burst.unwrap_or(self.requests)
    }
}

/// Rate limit result
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining requests in the current window
    pub remaining: i64,
    /// Total limit
    pub limit: u32,
    /// When the limit resets (seconds from now)
    pub reset_after: u64,
    /// Retry after duration (for 429 responses)
    pub retry_after: Option<u64>,
}

impl RateLimitResult {
    /// Create an allowed result
    pub fn allowed(remaining: i64, limit: u32, reset_after: u64) -> Self {
        Self {
            allowed: true,
            remaining,
            limit,
            reset_after,
            retry_after: None,
        }
    }

    /// Create a denied result
    pub fn denied(limit: u32, retry_after: u64) -> Self {
        Self {
            allowed: false,
            remaining: 0,
            limit,
            reset_after: retry_after,
            retry_after: Some(retry_after),
        }
    }
}

/// Rate limit key components
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RateLimitKey {
    /// The dimension type
    pub dimension: RateLimitDimension,
    /// The identifier (e.g., IP address, user ID, etc.)
    pub identifier: String,
    /// Optional route path (for route-specific limits)
    pub route: Option<String>,
}

impl RateLimitKey {
    /// Create a new rate limit key
    pub fn new(dimension: RateLimitDimension, identifier: String) -> Self {
        Self {
            dimension,
            identifier,
            route: None,
        }
    }

    /// Create a rate limit key with route
    pub fn with_route(dimension: RateLimitDimension, identifier: String, route: String) -> Self {
        Self {
            dimension,
            identifier,
            route: Some(route),
        }
    }

    /// Convert to a Redis key
    pub fn to_redis_key(&self) -> String {
        let dim = match self.dimension {
            RateLimitDimension::Ip => "ip",
            RateLimitDimension::User => "user",
            RateLimitDimension::ApiKey => "apikey",
            RateLimitDimension::Route => "route",
        };

        if let Some(route) = &self.route {
            format!("gateway:ratelimit:{}:{}:{}", dim, self.identifier, route)
        } else {
            format!("gateway:ratelimit:{}:{}", dim, self.identifier)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_key_to_redis_key() {
        let key = RateLimitKey::new(RateLimitDimension::Ip, "192.168.1.1".to_string());
        assert_eq!(key.to_redis_key(), "gateway:ratelimit:ip:192.168.1.1");

        let key_with_route = RateLimitKey::with_route(
            RateLimitDimension::User,
            "user123".to_string(),
            "/api/users".to_string(),
        );
        assert_eq!(
            key_with_route.to_redis_key(),
            "gateway:ratelimit:user:user123:/api/users"
        );
    }

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 100,
            window_secs: 60,
            burst: None,
        };

        assert_eq!(config.burst_size(), 100);
        assert_eq!(config.window(), Duration::from_secs(60));
    }

    #[test]
    fn test_rate_limit_result() {
        let allowed = RateLimitResult::allowed(50, 100, 30);
        assert!(allowed.allowed);
        assert_eq!(allowed.remaining, 50);
        assert_eq!(allowed.limit, 100);

        let denied = RateLimitResult::denied(100, 30);
        assert!(!denied.allowed);
        assert_eq!(denied.remaining, 0);
        assert_eq!(denied.retry_after, Some(30));
    }
}

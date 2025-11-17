//! Rate limiting module
//!
//! This module provides both local (in-memory) and distributed (Redis-backed)
//! rate limiting capabilities using various algorithms:
//!
//! - **Token Bucket**: Smooth rate limiting with burst support
//! - **Sliding Window**: More accurate rate limiting
//! - **Fixed Window**: Simpler implementation
//!
//! # Features
//!
//! - Multiple rate limiting dimensions (IP, User, API Key, Route)
//! - Graceful fallback from Redis to local rate limiting
//! - Rate limit headers in responses (`X-RateLimit-*`)
//! - Configurable per route
//!
//! # Example
//!
//! ```rust,no_run
//! use gateway::rate_limit::{RateLimiterService, RateLimitConfig, RateLimitDimension};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = RateLimitConfig {
//!         dimension: RateLimitDimension::Ip,
//!         requests: 100,
//!         window_secs: 60,
//!         burst: None,
//!     };
//!
//!     // Create local-only rate limiter
//!     let service = RateLimiterService::local_only(config);
//!
//!     // Or create Redis-backed rate limiter
//!     // let service = RateLimiterService::with_redis(
//!     //     config,
//!     //     "redis://localhost:6379",
//!     //     RateLimitAlgorithm::SlidingWindow,
//!     // ).await.unwrap();
//! }
//! ```

pub mod local;
pub mod lua_scripts;
pub mod middleware;
pub mod redis;
pub mod service;
pub mod types;

// Re-export commonly used types
pub use middleware::{add_rate_limit_headers, rate_limit_middleware, RateLimitMiddleware};
pub use redis::RateLimitAlgorithm;
pub use service::RateLimiterService;
pub use types::{
    RateLimitConfig, RateLimitDimension, RateLimitKey, RateLimitResult,
};

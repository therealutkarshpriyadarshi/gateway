use super::service::RateLimiterService;
use super::types::{RateLimitConfig, RateLimitDimension, RateLimitKey};
use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, warn};

/// Rate limiting middleware state
#[derive(Clone)]
pub struct RateLimitMiddleware {
    /// The rate limiter service
    service: Arc<RateLimiterService>,
    /// Rate limit configurations
    configs: Vec<RateLimitConfig>,
}

impl RateLimitMiddleware {
    /// Create a new rate limiting middleware
    pub fn new(service: Arc<RateLimiterService>, configs: Vec<RateLimitConfig>) -> Self {
        Self { service, configs }
    }

    /// Apply rate limiting to a request
    pub async fn apply(
        &self,
        request: Request,
        user_id: Option<String>,
        api_key: Option<String>,
    ) -> Result<Request, Response> {
        let path = request.uri().path().to_string();

        // Extract client IP
        let client_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Check rate limits for each configured dimension
        for config in &self.configs {
            let key = self.create_rate_limit_key(
                &config.dimension,
                &client_ip,
                user_id.as_deref(),
                api_key.as_deref(),
                &path,
            );

            if let Some(key) = key {
                let result = self.service.check_rate_limit(&key).await;

                if !result.allowed {
                    warn!(
                        "Rate limit exceeded for dimension {:?}, identifier: {}",
                        config.dimension, key.identifier
                    );

                    return Err(create_rate_limit_response(
                        result.limit,
                        result.remaining,
                        result.reset_after,
                        result.retry_after,
                    ));
                }

                debug!(
                    "Rate limit check passed for dimension {:?}, remaining: {}",
                    config.dimension, result.remaining
                );
            }
        }

        Ok(request)
    }

    /// Create a rate limit key based on the dimension
    fn create_rate_limit_key(
        &self,
        dimension: &RateLimitDimension,
        client_ip: &str,
        user_id: Option<&str>,
        api_key: Option<&str>,
        path: &str,
    ) -> Option<RateLimitKey> {
        match dimension {
            RateLimitDimension::Ip => Some(RateLimitKey::new(
                RateLimitDimension::Ip,
                client_ip.to_string(),
            )),
            RateLimitDimension::User => {
                user_id.map(|id| RateLimitKey::new(RateLimitDimension::User, id.to_string()))
            }
            RateLimitDimension::ApiKey => {
                api_key.map(|key| RateLimitKey::new(RateLimitDimension::ApiKey, key.to_string()))
            }
            RateLimitDimension::Route => Some(RateLimitKey::with_route(
                RateLimitDimension::Route,
                client_ip.to_string(),
                path.to_string(),
            )),
        }
    }
}

/// Create a 429 Too Many Requests response with rate limit headers
fn create_rate_limit_response(
    limit: u32,
    remaining: i64,
    reset_after: u64,
    retry_after: Option<u64>,
) -> Response {
    let mut headers = HeaderMap::new();

    headers.insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&limit.to_string()).unwrap(),
    );
    headers.insert(
        "X-RateLimit-Remaining",
        HeaderValue::from_str(&remaining.to_string()).unwrap(),
    );
    headers.insert(
        "X-RateLimit-Reset",
        HeaderValue::from_str(&reset_after.to_string()).unwrap(),
    );

    if let Some(retry) = retry_after {
        headers.insert(
            "Retry-After",
            HeaderValue::from_str(&retry.to_string()).unwrap(),
        );
    }

    let body = serde_json::json!({
        "error": "Rate limit exceeded",
        "status": 429,
        "limit": limit,
        "remaining": remaining,
        "reset_after": reset_after,
        "retry_after": retry_after,
    });

    (StatusCode::TOO_MANY_REQUESTS, headers, body.to_string()).into_response()
}

/// Axum middleware function for rate limiting
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    // Store the client IP in request extensions
    request.extensions_mut().insert(ConnectInfo(addr));

    // Extract user ID from request extensions (set by auth middleware)
    let user_id = request.extensions().get::<String>().map(|s| s.to_string());

    // Extract API key from headers
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Get rate limiter from request extensions and clone it
    let rate_limiter = request.extensions().get::<RateLimitMiddleware>().cloned();

    if let Some(limiter) = rate_limiter {
        match limiter.apply(request, user_id, api_key).await {
            Ok(req) => next.run(req).await,
            Err(response) => response,
        }
    } else {
        // No rate limiter configured, allow the request
        next.run(request).await
    }
}

/// Add rate limit headers to successful responses
pub fn add_rate_limit_headers(
    mut response: Response,
    limit: u32,
    remaining: i64,
    reset_after: u64,
) -> Response {
    let headers = response.headers_mut();

    headers.insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&limit.to_string()).unwrap(),
    );
    headers.insert(
        "X-RateLimit-Remaining",
        HeaderValue::from_str(&remaining.to_string()).unwrap(),
    );
    headers.insert(
        "X-RateLimit-Reset",
        HeaderValue::from_str(&reset_after.to_string()).unwrap(),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::service::RateLimiterService;

    #[test]
    fn test_create_rate_limit_key() {
        let config = RateLimitConfig {
            dimension: RateLimitDimension::Ip,
            requests: 100,
            window_secs: 60,
            burst: None,
        };

        let service = RateLimiterService::local_only(config.clone());
        let middleware = RateLimitMiddleware::new(Arc::new(service), vec![config]);

        // Test IP dimension
        let key = middleware.create_rate_limit_key(
            &RateLimitDimension::Ip,
            "192.168.1.1",
            None,
            None,
            "/api/test",
        );
        assert!(key.is_some());
        assert_eq!(key.unwrap().identifier, "192.168.1.1");

        // Test User dimension (no user)
        let key = middleware.create_rate_limit_key(
            &RateLimitDimension::User,
            "192.168.1.1",
            None,
            None,
            "/api/test",
        );
        assert!(key.is_none());

        // Test User dimension (with user)
        let key = middleware.create_rate_limit_key(
            &RateLimitDimension::User,
            "192.168.1.1",
            Some("user123"),
            None,
            "/api/test",
        );
        assert!(key.is_some());
        assert_eq!(key.unwrap().identifier, "user123");

        // Test Route dimension
        let key = middleware.create_rate_limit_key(
            &RateLimitDimension::Route,
            "192.168.1.1",
            None,
            None,
            "/api/test",
        );
        assert!(key.is_some());
        let key = key.unwrap();
        assert_eq!(key.identifier, "192.168.1.1");
        assert_eq!(key.route, Some("/api/test".to_string()));
    }

    #[test]
    fn test_rate_limit_response() {
        let response = create_rate_limit_response(100, 0, 30, Some(30));

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let headers = response.headers();
        assert_eq!(headers.get("X-RateLimit-Limit").unwrap(), "100");
        assert_eq!(headers.get("X-RateLimit-Remaining").unwrap(), "0");
        assert_eq!(headers.get("Retry-After").unwrap(), "30");
    }
}

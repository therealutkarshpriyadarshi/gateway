use crate::error::Result;
use axum::body::Body;
use axum::http::{HeaderMap, Response, StatusCode};
use bytes::Bytes;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable caching
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Maximum number of entries in cache
    #[serde(default = "default_max_capacity")]
    pub max_capacity: u64,
    /// Time-to-live for cache entries in seconds
    #[serde(default = "default_ttl_secs")]
    pub ttl_secs: u64,
    /// List of HTTP methods to cache (e.g., ["GET", "HEAD"])
    #[serde(default = "default_cacheable_methods")]
    pub cacheable_methods: Vec<String>,
    /// List of status codes to cache (e.g., [200, 301])
    #[serde(default = "default_cacheable_status_codes")]
    pub cacheable_status_codes: Vec<u16>,
    /// Headers to include in cache key (in addition to path and method)
    #[serde(default)]
    pub key_headers: Vec<String>,
    /// Whether to cache responses with Set-Cookie headers
    #[serde(default)]
    pub cache_with_cookies: bool,
}

fn default_enabled() -> bool {
    false
}

fn default_max_capacity() -> u64 {
    1000
}

fn default_ttl_secs() -> u64 {
    300 // 5 minutes
}

fn default_cacheable_methods() -> Vec<String> {
    vec!["GET".to_string(), "HEAD".to_string()]
}

fn default_cacheable_status_codes() -> Vec<u16> {
    vec![200, 301, 302, 404]
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            max_capacity: default_max_capacity(),
            ttl_secs: default_ttl_secs(),
            cacheable_methods: default_cacheable_methods(),
            cacheable_status_codes: default_cacheable_status_codes(),
            key_headers: vec![],
            cache_with_cookies: false,
        }
    }
}

/// Cached response entry
#[derive(Clone, Debug)]
pub struct CachedResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl CachedResponse {
    /// Convert to Axum response
    pub fn to_response(&self) -> Response<Body> {
        let mut response = Response::builder().status(self.status);

        // Copy headers
        for (name, value) in self.headers.iter() {
            response = response.header(name, value);
        }

        // Add cache hit header
        response = response.header("X-Cache", "HIT");

        response
            .body(Body::from(self.body.clone()))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            })
    }
}

/// Cache key for requests
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CacheKey {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
}

impl CacheKey {
    /// Create a new cache key
    pub fn new(
        method: String,
        path: String,
        query: Option<String>,
        request_headers: &HeaderMap,
        key_headers: &[String],
    ) -> Self {
        // Extract specified headers for cache key
        let mut headers = Vec::new();
        for header_name in key_headers {
            if let Ok(name) = header_name.parse::<axum::http::HeaderName>() {
                if let Some(value) = request_headers.get(&name) {
                    if let Ok(value_str) = value.to_str() {
                        headers.push((header_name.clone(), value_str.to_string()));
                    }
                }
            }
        }
        headers.sort(); // Ensure consistent ordering

        Self {
            method,
            path,
            query,
            headers,
        }
    }
}

/// Cache service for storing and retrieving responses
#[derive(Debug)]
pub struct CacheService {
    config: CacheConfig,
    cache: Arc<Cache<CacheKey, CachedResponse>>,
}

impl CacheService {
    /// Create a new cache service
    pub fn new(config: CacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(Duration::from_secs(config.ttl_secs))
            .build();

        info!(
            max_capacity = config.max_capacity,
            ttl_secs = config.ttl_secs,
            "Initialized cache service"
        );

        Self {
            config,
            cache: Arc::new(cache),
        }
    }

    /// Check if a method is cacheable
    pub fn is_cacheable_method(&self, method: &str) -> bool {
        self.config
            .cacheable_methods
            .iter()
            .any(|m| m.eq_ignore_ascii_case(method))
    }

    /// Check if a status code is cacheable
    pub fn is_cacheable_status(&self, status: u16) -> bool {
        self.config.cacheable_status_codes.contains(&status)
    }

    /// Check if response can be cached based on headers
    pub fn is_response_cacheable(&self, headers: &HeaderMap) -> bool {
        // Don't cache if response has Set-Cookie header (unless configured to do so)
        if !self.config.cache_with_cookies && headers.contains_key("set-cookie") {
            return false;
        }

        // Check Cache-Control header
        if let Some(cache_control) = headers.get("cache-control") {
            if let Ok(cc_str) = cache_control.to_str() {
                let cc_lower = cc_str.to_lowercase();
                // Don't cache if no-store, private, or no-cache is set
                if cc_lower.contains("no-store")
                    || cc_lower.contains("private")
                    || cc_lower.contains("no-cache")
                {
                    return false;
                }
            }
        }

        true
    }

    /// Get a cached response
    pub async fn get(&self, key: &CacheKey) -> Option<CachedResponse> {
        let cached = self.cache.get(key).await;
        if cached.is_some() {
            debug!(
                method = %key.method,
                path = %key.path,
                "Cache hit"
            );
        }
        cached
    }

    /// Store a response in cache
    pub async fn put(
        &self,
        key: CacheKey,
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<()> {
        // Check if method is cacheable
        if !self.is_cacheable_method(&key.method) {
            return Ok(());
        }

        // Check if status code is cacheable
        if !self.is_cacheable_status(status.as_u16()) {
            return Ok(());
        }

        // Check if response headers allow caching
        if !self.is_response_cacheable(&headers) {
            return Ok(());
        }

        let cached = CachedResponse {
            status,
            headers,
            body,
        };

        self.cache.insert(key.clone(), cached).await;

        debug!(
            method = %key.method,
            path = %key.path,
            status = %status.as_u16(),
            "Cached response"
        );

        Ok(())
    }

    /// Invalidate cache entry
    pub async fn invalidate(&self, key: &CacheKey) {
        self.cache.invalidate(key).await;
        debug!(
            method = %key.method,
            path = %key.path,
            "Invalidated cache entry"
        );
    }

    /// Clear all cache entries
    pub async fn clear(&self) {
        self.cache.invalidate_all();
        info!("Cleared all cache entries");
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.entry_count(),
            weighted_size: self.cache.weighted_size(),
        }
    }

    /// Get cache key headers configuration
    pub fn key_headers(&self) -> &[String] {
        &self.config.key_headers
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub entry_count: u64,
    pub weighted_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_default_cache_config() {
        let config = CacheConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_capacity, 1000);
        assert_eq!(config.ttl_secs, 300);
        assert!(config.cacheable_methods.contains(&"GET".to_string()));
    }

    #[test]
    fn test_cache_service_creation() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);
        assert!(service.is_cacheable_method("GET"));
        assert!(service.is_cacheable_method("get")); // case insensitive
        assert!(!service.is_cacheable_method("POST"));
    }

    #[test]
    fn test_cacheable_status_codes() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);
        assert!(service.is_cacheable_status(200));
        assert!(service.is_cacheable_status(301));
        assert!(service.is_cacheable_status(404));
        assert!(!service.is_cacheable_status(500));
    }

    #[test]
    fn test_response_cacheable_with_set_cookie() {
        let config = CacheConfig {
            cache_with_cookies: false,
            ..Default::default()
        };
        let service = CacheService::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("set-cookie", HeaderValue::from_static("session=abc"));

        assert!(!service.is_response_cacheable(&headers));
    }

    #[test]
    fn test_response_cacheable_with_no_store() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("cache-control", HeaderValue::from_static("no-store"));

        assert!(!service.is_response_cacheable(&headers));
    }

    #[test]
    fn test_response_cacheable_with_private() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("cache-control", HeaderValue::from_static("private"));

        assert!(!service.is_response_cacheable(&headers));
    }

    #[test]
    fn test_response_cacheable() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);

        let headers = HeaderMap::new();
        assert!(service.is_response_cacheable(&headers));

        let mut headers_with_public = HeaderMap::new();
        headers_with_public.insert("cache-control", HeaderValue::from_static("public, max-age=3600"));
        assert!(service.is_response_cacheable(&headers_with_public));
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);

        let key = CacheKey::new(
            "GET".to_string(),
            "/test".to_string(),
            None,
            &HeaderMap::new(),
            &[],
        );

        let headers = HeaderMap::new();
        let body = Bytes::from("test response");

        service
            .put(key.clone(), StatusCode::OK, headers.clone(), body.clone())
            .await
            .unwrap();

        let cached = service.get(&key).await;
        assert!(cached.is_some());

        let cached_response = cached.unwrap();
        assert_eq!(cached_response.status, StatusCode::OK);
        assert_eq!(cached_response.body, body);
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let config = CacheConfig::default();
        let service = CacheService::new(config);

        let key = CacheKey::new(
            "GET".to_string(),
            "/test".to_string(),
            None,
            &HeaderMap::new(),
            &[],
        );

        service
            .put(
                key.clone(),
                StatusCode::OK,
                HeaderMap::new(),
                Bytes::from("test"),
            )
            .await
            .unwrap();

        assert!(service.get(&key).await.is_some());

        service.invalidate(&key).await;

        assert!(service.get(&key).await.is_none());
    }

    #[test]
    fn test_cache_key_with_headers() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert("accept-language", HeaderValue::from_static("en-US"));
        request_headers.insert("authorization", HeaderValue::from_static("Bearer token"));

        let key1 = CacheKey::new(
            "GET".to_string(),
            "/test".to_string(),
            None,
            &request_headers,
            &["Accept-Language".to_string()],
        );

        let key2 = CacheKey::new(
            "GET".to_string(),
            "/test".to_string(),
            None,
            &request_headers,
            &[],
        );

        // Keys should be different when including headers
        assert_ne!(key1, key2);
        assert_eq!(key1.headers.len(), 1);
        assert_eq!(key2.headers.len(), 0);
    }
}

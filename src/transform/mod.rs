use crate::error::{GatewayError, Result};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, warn};

/// Request/Response transformation configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransformConfig {
    /// Request transformations
    #[serde(default)]
    pub request: Option<RequestTransform>,
    /// Response transformations
    #[serde(default)]
    pub response: Option<ResponseTransform>,
}

/// Request transformation options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestTransform {
    /// Headers to add (will not override existing)
    #[serde(default)]
    pub add_headers: HashMap<String, String>,
    /// Headers to set (will override existing)
    #[serde(default)]
    pub set_headers: HashMap<String, String>,
    /// Headers to remove
    #[serde(default)]
    pub remove_headers: Vec<String>,
    /// URL path rewrites (regex pattern -> replacement)
    #[serde(default)]
    pub path_rewrites: Vec<PathRewrite>,
    /// Query parameter transformations
    #[serde(default)]
    pub query_params: Option<QueryParamTransform>,
}

/// Response transformation options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResponseTransform {
    /// Headers to add (will not override existing)
    #[serde(default)]
    pub add_headers: HashMap<String, String>,
    /// Headers to set (will override existing)
    #[serde(default)]
    pub set_headers: HashMap<String, String>,
    /// Headers to remove
    #[serde(default)]
    pub remove_headers: Vec<String>,
}

/// Path rewrite rule using regex
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRewrite {
    /// Regular expression pattern to match
    pub pattern: String,
    /// Replacement string (can use capture groups like $1, $2)
    pub replacement: String,
}

/// Query parameter transformations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryParamTransform {
    /// Query parameters to add (will not override existing)
    #[serde(default)]
    pub add: HashMap<String, String>,
    /// Query parameters to set (will override existing)
    #[serde(default)]
    pub set: HashMap<String, String>,
    /// Query parameters to remove
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Header transformation service
#[derive(Debug)]
pub struct TransformService {
    config: TransformConfig,
    path_rewrite_cache: Vec<(Regex, String)>,
}

impl TransformService {
    /// Create a new transform service from configuration
    pub fn new(config: TransformConfig) -> Result<Self> {
        // Pre-compile regex patterns for path rewrites
        let mut path_rewrite_cache = Vec::new();
        if let Some(request) = &config.request {
            for rewrite in &request.path_rewrites {
                let regex = Regex::new(&rewrite.pattern).map_err(|e| {
                    GatewayError::Config(format!(
                        "Invalid path rewrite regex '{}': {}",
                        rewrite.pattern, e
                    ))
                })?;
                path_rewrite_cache.push((regex, rewrite.replacement.clone()));
            }
        }

        Ok(Self {
            config,
            path_rewrite_cache,
        })
    }

    /// Transform request headers
    pub fn transform_request_headers(&self, headers: &mut HeaderMap) -> Result<()> {
        if let Some(request) = &self.config.request {
            // Remove headers
            for header_name in &request.remove_headers {
                if let Ok(name) = HeaderName::from_str(header_name) {
                    headers.remove(&name);
                    debug!(header = %header_name, "Removed request header");
                } else {
                    warn!(header = %header_name, "Invalid header name for removal");
                }
            }

            // Add headers (only if not already present)
            for (key, value) in &request.add_headers {
                if let (Ok(name), Ok(val)) =
                    (HeaderName::from_str(key), HeaderValue::from_str(value))
                {
                    if !headers.contains_key(&name) {
                        headers.insert(name, val);
                        debug!(header = %key, value = %value, "Added request header");
                    }
                } else {
                    warn!(header = %key, "Invalid header name or value for add");
                }
            }

            // Set headers (override existing)
            for (key, value) in &request.set_headers {
                if let (Ok(name), Ok(val)) =
                    (HeaderName::from_str(key), HeaderValue::from_str(value))
                {
                    headers.insert(name, val);
                    debug!(header = %key, value = %value, "Set request header");
                } else {
                    warn!(header = %key, "Invalid header name or value for set");
                }
            }
        }

        Ok(())
    }

    /// Transform response headers
    pub fn transform_response_headers(&self, headers: &mut HeaderMap) -> Result<()> {
        if let Some(response) = &self.config.response {
            // Remove headers
            for header_name in &response.remove_headers {
                if let Ok(name) = HeaderName::from_str(header_name) {
                    headers.remove(&name);
                    debug!(header = %header_name, "Removed response header");
                } else {
                    warn!(header = %header_name, "Invalid header name for removal");
                }
            }

            // Add headers (only if not already present)
            for (key, value) in &response.add_headers {
                if let (Ok(name), Ok(val)) =
                    (HeaderName::from_str(key), HeaderValue::from_str(value))
                {
                    if !headers.contains_key(&name) {
                        headers.insert(name, val);
                        debug!(header = %key, value = %value, "Added response header");
                    }
                } else {
                    warn!(header = %key, "Invalid header name or value for add");
                }
            }

            // Set headers (override existing)
            for (key, value) in &response.set_headers {
                if let (Ok(name), Ok(val)) =
                    (HeaderName::from_str(key), HeaderValue::from_str(value))
                {
                    headers.insert(name, val);
                    debug!(header = %key, value = %value, "Set response header");
                } else {
                    warn!(header = %key, "Invalid header name or value for set");
                }
            }
        }

        Ok(())
    }

    /// Transform request path using configured rewrites
    pub fn transform_path(&self, path: &str) -> String {
        let mut transformed = path.to_string();

        for (regex, replacement) in &self.path_rewrite_cache {
            if regex.is_match(&transformed) {
                let new_path = regex.replace(&transformed, replacement.as_str()).to_string();
                debug!(
                    original = %transformed,
                    rewritten = %new_path,
                    pattern = %regex.as_str(),
                    "Path rewritten"
                );
                transformed = new_path;
                // Only apply first matching rewrite
                break;
            }
        }

        transformed
    }

    /// Transform query parameters
    pub fn transform_query_params(&self, query: &str) -> String {
        if let Some(request) = &self.config.request {
            if let Some(query_transform) = &request.query_params {
                // Parse existing query parameters
                let mut params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect();

                // Remove parameters
                for key in &query_transform.remove {
                    params.remove(key);
                }

                // Add parameters (only if not present)
                for (key, value) in &query_transform.add {
                    params.entry(key.clone()).or_insert_with(|| value.clone());
                }

                // Set parameters (override existing)
                for (key, value) in &query_transform.set {
                    params.insert(key.clone(), value.clone());
                }

                // Rebuild query string
                return url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(params)
                    .finish();
            }
        }

        query.to_string()
    }

    /// Check if there are any request transformations configured
    pub fn has_request_transform(&self) -> bool {
        self.config.request.is_some()
    }

    /// Check if there are any response transformations configured
    pub fn has_response_transform(&self) -> bool {
        self.config.response.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_add_transformation() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                add_headers: vec![("X-Custom-Header".to_string(), "value".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();
        let mut headers = HeaderMap::new();

        service.transform_request_headers(&mut headers).unwrap();

        assert_eq!(
            headers.get("X-Custom-Header").unwrap(),
            HeaderValue::from_str("value").unwrap()
        );
    }

    #[test]
    fn test_header_add_does_not_override() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                add_headers: vec![("X-Custom-Header".to_string(), "new-value".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("X-Custom-Header").unwrap(),
            HeaderValue::from_str("existing-value").unwrap(),
        );

        service.transform_request_headers(&mut headers).unwrap();

        // Should not override existing header
        assert_eq!(
            headers.get("X-Custom-Header").unwrap(),
            HeaderValue::from_str("existing-value").unwrap()
        );
    }

    #[test]
    fn test_header_set_overrides() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                set_headers: vec![("X-Custom-Header".to_string(), "new-value".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("X-Custom-Header").unwrap(),
            HeaderValue::from_str("existing-value").unwrap(),
        );

        service.transform_request_headers(&mut headers).unwrap();

        // Should override existing header
        assert_eq!(
            headers.get("X-Custom-Header").unwrap(),
            HeaderValue::from_str("new-value").unwrap()
        );
    }

    #[test]
    fn test_header_removal() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                remove_headers: vec!["X-Remove-Me".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("X-Remove-Me").unwrap(),
            HeaderValue::from_str("value").unwrap(),
        );

        service.transform_request_headers(&mut headers).unwrap();

        assert!(headers.get("X-Remove-Me").is_none());
    }

    #[test]
    fn test_path_rewrite() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                path_rewrites: vec![PathRewrite {
                    pattern: r"^/old/(.*)$".to_string(),
                    replacement: "/new/$1".to_string(),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();

        assert_eq!(
            service.transform_path("/old/resource"),
            "/new/resource"
        );
        assert_eq!(service.transform_path("/other/path"), "/other/path");
    }

    #[test]
    fn test_path_rewrite_multiple_rules() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                path_rewrites: vec![
                    PathRewrite {
                        pattern: r"^/v1/(.*)$".to_string(),
                        replacement: "/api/v1/$1".to_string(),
                    },
                    PathRewrite {
                        pattern: r"^/v2/(.*)$".to_string(),
                        replacement: "/api/v2/$1".to_string(),
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();

        assert_eq!(service.transform_path("/v1/users"), "/api/v1/users");
        assert_eq!(service.transform_path("/v2/products"), "/api/v2/products");
    }

    #[test]
    fn test_query_param_transformation() {
        let config = TransformConfig {
            request: Some(RequestTransform {
                query_params: Some(QueryParamTransform {
                    add: vec![("new_param".to_string(), "value".to_string())]
                        .into_iter()
                        .collect(),
                    remove: vec!["remove_me".to_string()],
                    set: vec![("override".to_string(), "new_value".to_string())]
                        .into_iter()
                        .collect(),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();

        let transformed = service.transform_query_params("existing=value&remove_me=x&override=old");

        // Parse transformed query to check
        let params: HashMap<String, String> = url::form_urlencoded::parse(transformed.as_bytes())
            .into_owned()
            .collect();

        assert!(params.contains_key("existing"));
        assert!(params.contains_key("new_param"));
        assert!(!params.contains_key("remove_me"));
        assert_eq!(params.get("override").unwrap(), "new_value");
    }

    #[test]
    fn test_response_header_transformation() {
        let config = TransformConfig {
            response: Some(ResponseTransform {
                set_headers: vec![("X-Powered-By".to_string(), "Rust Gateway".to_string())]
                    .into_iter()
                    .collect(),
                remove_headers: vec!["Server".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = TransformService::new(config).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("Server").unwrap(),
            HeaderValue::from_str("nginx").unwrap(),
        );

        service.transform_response_headers(&mut headers).unwrap();

        assert!(headers.get("Server").is_none());
        assert_eq!(
            headers.get("X-Powered-By").unwrap(),
            HeaderValue::from_str("Rust Gateway").unwrap()
        );
    }
}

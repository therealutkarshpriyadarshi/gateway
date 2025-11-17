use crate::error::{GatewayError, Result};
use axum::http::{HeaderValue, Method};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::debug;

/// CORS configuration for routes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins (use ["*"] for all origins)
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods
    #[serde(default = "default_methods")]
    pub allowed_methods: Vec<String>,
    /// Allowed headers
    #[serde(default = "default_headers")]
    pub allowed_headers: Vec<String>,
    /// Exposed headers
    #[serde(default)]
    pub exposed_headers: Vec<String>,
    /// Allow credentials
    #[serde(default)]
    pub allow_credentials: bool,
    /// Max age for preflight cache in seconds
    #[serde(default = "default_max_age")]
    pub max_age_secs: u64,
}

fn default_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "PATCH".to_string(),
        "OPTIONS".to_string(),
    ]
}

fn default_headers() -> Vec<String> {
    vec![
        "Content-Type".to_string(),
        "Authorization".to_string(),
        "X-API-Key".to_string(),
    ]
}

fn default_max_age() -> u64 {
    3600
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: default_methods(),
            allowed_headers: default_headers(),
            exposed_headers: vec![],
            allow_credentials: false,
            max_age_secs: default_max_age(),
        }
    }
}

impl CorsConfig {
    /// Create a permissive CORS configuration (allows all origins, methods, headers)
    pub fn permissive() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "PATCH".to_string(),
                "OPTIONS".to_string(),
                "HEAD".to_string(),
            ],
            allowed_headers: vec!["*".to_string()],
            exposed_headers: vec![],
            allow_credentials: false,
            max_age_secs: 86400, // 24 hours
        }
    }

    /// Create a restrictive CORS configuration (specific origins only)
    pub fn restrictive(origins: Vec<String>) -> Self {
        Self {
            allowed_origins: origins,
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
            exposed_headers: vec![],
            allow_credentials: true,
            max_age_secs: 600, // 10 minutes
        }
    }

    /// Build a CorsLayer from this configuration
    pub fn build_layer(&self) -> Result<CorsLayer> {
        let mut cors = CorsLayer::new();

        // Configure allowed origins
        if self.allowed_origins.len() == 1 && self.allowed_origins[0] == "*" {
            cors = cors.allow_origin(AllowOrigin::any());
            debug!("CORS: Allowing all origins");
        } else {
            let origins: std::result::Result<Vec<HeaderValue>, _> = self
                .allowed_origins
                .iter()
                .map(|o| HeaderValue::from_str(o))
                .collect();

            match origins {
                Ok(origin_values) => {
                    cors = cors.allow_origin(origin_values);
                    debug!(origins = ?self.allowed_origins, "CORS: Configured allowed origins");
                }
                Err(e) => {
                    return Err(GatewayError::Config(format!(
                        "Invalid CORS origin value: {}",
                        e
                    )));
                }
            }
        }

        // Configure allowed methods
        let methods: std::result::Result<Vec<Method>, _> = self
            .allowed_methods
            .iter()
            .map(|m| Method::from_bytes(m.as_bytes()))
            .collect();

        match methods {
            Ok(method_values) => {
                cors = cors.allow_methods(method_values);
                debug!(methods = ?self.allowed_methods, "CORS: Configured allowed methods");
            }
            Err(e) => {
                return Err(GatewayError::Config(format!(
                    "Invalid CORS method: {}",
                    e
                )));
            }
        }

        // Configure allowed headers
        if self.allowed_headers.len() == 1 && self.allowed_headers[0] == "*" {
            cors = cors.allow_headers(tower_http::cors::Any);
            debug!("CORS: Allowing all headers");
        } else {
            let headers: std::result::Result<Vec<_>, _> = self
                .allowed_headers
                .iter()
                .map(|h| h.parse())
                .collect();

            match headers {
                Ok(header_values) => {
                    cors = cors.allow_headers(header_values);
                    debug!(headers = ?self.allowed_headers, "CORS: Configured allowed headers");
                }
                Err(e) => {
                    return Err(GatewayError::Config(format!(
                        "Invalid CORS header name: {}",
                        e
                    )));
                }
            }
        }

        // Configure exposed headers
        if !self.exposed_headers.is_empty() {
            let headers: std::result::Result<Vec<_>, _> = self
                .exposed_headers
                .iter()
                .map(|h| h.parse())
                .collect();

            match headers {
                Ok(header_values) => {
                    cors = cors.expose_headers(header_values);
                    debug!(headers = ?self.exposed_headers, "CORS: Configured exposed headers");
                }
                Err(e) => {
                    return Err(GatewayError::Config(format!(
                        "Invalid exposed header name: {}",
                        e
                    )));
                }
            }
        }

        // Configure credentials
        if self.allow_credentials {
            cors = cors.allow_credentials(true);
            debug!("CORS: Allowing credentials");
        }

        // Configure max age
        cors = cors.max_age(Duration::from_secs(self.max_age_secs));
        debug!(max_age_secs = self.max_age_secs, "CORS: Configured max age");

        Ok(cors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cors_config() {
        let config = CorsConfig::default();
        assert_eq!(config.allowed_origins, vec!["*"]);
        assert!(config.allowed_methods.contains(&"GET".to_string()));
        assert!(config.allowed_headers.contains(&"Content-Type".to_string()));
        assert!(!config.allow_credentials);
    }

    #[test]
    fn test_permissive_cors_config() {
        let config = CorsConfig::permissive();
        assert_eq!(config.allowed_origins, vec!["*"]);
        assert_eq!(config.allowed_headers, vec!["*"]);
        assert!(!config.allow_credentials);
        assert_eq!(config.max_age_secs, 86400);
    }

    #[test]
    fn test_restrictive_cors_config() {
        let origins = vec!["https://example.com".to_string()];
        let config = CorsConfig::restrictive(origins.clone());
        assert_eq!(config.allowed_origins, origins);
        assert!(config.allow_credentials);
        assert_eq!(config.allowed_methods.len(), 2);
    }

    #[test]
    fn test_build_layer_with_wildcard_origin() {
        let config = CorsConfig {
            allowed_origins: vec!["*".to_string()],
            ..Default::default()
        };

        let result = config.build_layer();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_layer_with_specific_origins() {
        let config = CorsConfig {
            allowed_origins: vec![
                "https://example.com".to_string(),
                "https://app.example.com".to_string(),
            ],
            ..Default::default()
        };

        let result = config.build_layer();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_layer_with_all_headers() {
        let config = CorsConfig {
            allowed_origins: vec!["*".to_string()],
            allowed_headers: vec!["*".to_string()],
            ..Default::default()
        };

        let result = config.build_layer();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_layer_with_specific_headers() {
        let config = CorsConfig {
            allowed_origins: vec!["*".to_string()],
            allowed_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
            exposed_headers: vec!["X-Request-ID".to_string()],
            ..Default::default()
        };

        let result = config.build_layer();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_layer_with_credentials() {
        let config = CorsConfig {
            allowed_origins: vec!["https://example.com".to_string()],
            allow_credentials: true,
            ..Default::default()
        };

        let result = config.build_layer();
        assert!(result.is_ok());
    }

}

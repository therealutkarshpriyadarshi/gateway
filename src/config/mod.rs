use crate::error::{GatewayError, Result};
use crate::rate_limit::types::{RateLimitConfig, RateLimitDimension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Route definitions
    pub routes: Vec<RouteConfig>,
    /// Authentication configuration
    #[serde(default)]
    pub auth: Option<AuthConfig>,
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limiting: Option<GlobalRateLimitConfig>,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server host address
    #[serde(default = "default_host")]
    pub host: String,
    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

/// Route configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteConfig {
    /// Route path pattern (e.g., "/api/users/:id")
    pub path: String,
    /// Backend service URL
    pub backend: String,
    /// Allowed HTTP methods (if empty, all methods allowed)
    #[serde(default)]
    pub methods: Vec<String>,
    /// Whether to strip the prefix when forwarding
    #[serde(default)]
    pub strip_prefix: bool,
    /// Route description
    #[serde(default)]
    pub description: String,
    /// Authentication requirement for this route
    #[serde(default)]
    pub auth: Option<RouteAuthConfig>,
    /// Rate limiting for this route
    #[serde(default)]
    pub rate_limit: Option<Vec<RateLimitConfig>>,
}

/// Authentication configuration for a route
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteAuthConfig {
    /// Whether authentication is required
    #[serde(default = "default_true")]
    pub required: bool,
    /// Allowed authentication methods
    #[serde(default)]
    pub methods: Vec<AuthMethod>,
}

/// Authentication method types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    Jwt,
    ApiKey,
}

/// Global authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// JWT configuration
    pub jwt: Option<JwtConfig>,
    /// API key configuration
    pub api_key: Option<ApiKeyConfig>,
}

/// JWT authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// Secret key for HS256 (if using symmetric encryption)
    pub secret: Option<String>,
    /// Public key for RS256 (if using asymmetric encryption)
    pub public_key: Option<String>,
    /// Algorithm to use (HS256 or RS256)
    #[serde(default = "default_jwt_algorithm")]
    pub algorithm: String,
    /// Issuer to validate
    pub issuer: Option<String>,
    /// Audience to validate
    pub audience: Option<String>,
}

/// API key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// Header name for API key
    #[serde(default = "default_api_key_header")]
    pub header: String,
    /// In-memory API keys (key -> description)
    #[serde(default)]
    pub keys: std::collections::HashMap<String, String>,
    /// Redis configuration for distributed key storage
    pub redis: Option<RedisConfig>,
}

/// Redis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    /// Key prefix for API keys
    #[serde(default = "default_redis_prefix")]
    pub prefix: String,
}

/// Global rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRateLimitConfig {
    /// Enable rate limiting globally
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Global rate limit rules (applied to all routes)
    #[serde(default)]
    pub global: Vec<RateLimitConfig>,
    /// Redis configuration for distributed rate limiting
    pub redis: Option<RateLimitRedisConfig>,
    /// Algorithm to use for Redis rate limiting
    #[serde(default = "default_rate_limit_algorithm")]
    pub algorithm: String,
}

/// Redis configuration for rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRedisConfig {
    /// Redis connection URL
    pub url: String,
}

fn default_true() -> bool {
    true
}

fn default_jwt_algorithm() -> String {
    "HS256".to_string()
}

fn default_api_key_header() -> String {
    "X-API-Key".to_string()
}

fn default_redis_prefix() -> String {
    "gateway:apikey:".to_string()
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_timeout() -> u64 {
    30
}

fn default_rate_limit_algorithm() -> String {
    "sliding_window".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            timeout_secs: default_timeout(),
        }
    }
}

impl GatewayConfig {
    /// Load configuration from a YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| GatewayError::Config(format!("Failed to read config file: {}", e)))?;

        Self::from_yaml(&content)
    }

    /// Parse configuration from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml)
            .map_err(|e| GatewayError::Config(format!("Failed to parse config: {}", e)))
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate routes
        for route in &self.routes {
            if route.path.is_empty() {
                return Err(GatewayError::InvalidRoute(
                    "Route path cannot be empty".to_string(),
                ));
            }

            if route.backend.is_empty() {
                return Err(GatewayError::InvalidRoute(format!(
                    "Backend URL cannot be empty for route: {}",
                    route.path
                )));
            }

            // Validate backend URL
            if !route.backend.starts_with("http://") && !route.backend.starts_with("https://") {
                return Err(GatewayError::InvalidRoute(format!(
                    "Backend URL must start with http:// or https:// for route: {}",
                    route.path
                )));
            }

            // Validate methods
            for method in &route.methods {
                let method_upper = method.to_uppercase();
                if !["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]
                    .contains(&method_upper.as_str())
                {
                    return Err(GatewayError::InvalidRoute(format!(
                        "Invalid HTTP method '{}' for route: {}",
                        method, route.path
                    )));
                }
            }

            // Validate rate limits
            if let Some(rate_limits) = &route.rate_limit {
                for limit in rate_limits {
                    if limit.requests == 0 {
                        return Err(GatewayError::Config(format!(
                            "Rate limit requests must be > 0 for route: {}",
                            route.path
                        )));
                    }
                    if limit.window_secs == 0 {
                        return Err(GatewayError::Config(format!(
                            "Rate limit window must be > 0 for route: {}",
                            route.path
                        )));
                    }
                }
            }
        }

        // Validate global rate limits
        if let Some(rate_limiting) = &self.rate_limiting {
            for limit in &rate_limiting.global {
                if limit.requests == 0 {
                    return Err(GatewayError::Config(
                        "Global rate limit requests must be > 0".to_string(),
                    ));
                }
                if limit.window_secs == 0 {
                    return Err(GatewayError::Config(
                        "Global rate limit window must be > 0".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Create a default configuration for testing
    pub fn default_config() -> Self {
        Self {
            server: ServerConfig::default(),
            routes: vec![],
            auth: None,
            rate_limiting: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_config() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 8080
  timeout_secs: 30

routes:
  - path: "/api/users"
    backend: "http://localhost:3000"
    methods: ["GET", "POST"]
    description: "User service"
  - path: "/api/orders/:id"
    backend: "http://localhost:3001"
    methods: ["GET"]
    strip_prefix: true
"#;

        let config = GatewayConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.routes.len(), 2);
        assert_eq!(config.routes[0].path, "/api/users");
        assert_eq!(config.routes[0].methods, vec!["GET", "POST"]);
        assert_eq!(config.routes[1].strip_prefix, true);
    }

    #[test]
    fn test_default_values() {
        let yaml = r#"
server: {}
routes: []
"#;

        let config = GatewayConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.timeout_secs, 30);
    }

    #[test]
    fn test_validate_empty_path() {
        let config = GatewayConfig {
            server: ServerConfig::default(),
            routes: vec![RouteConfig {
                path: "".to_string(),
                backend: "http://localhost:3000".to_string(),
                methods: vec![],
                strip_prefix: false,
                description: "".to_string(),
                auth: None,
                rate_limit: None,
            }],
            auth: None,
            rate_limiting: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_backend() {
        let config = GatewayConfig {
            server: ServerConfig::default(),
            routes: vec![RouteConfig {
                path: "/api/test".to_string(),
                backend: "invalid-url".to_string(),
                methods: vec![],
                strip_prefix: false,
                description: "".to_string(),
                auth: None,
                rate_limit: None,
            }],
            auth: None,
            rate_limiting: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_method() {
        let config = GatewayConfig {
            server: ServerConfig::default(),
            routes: vec![RouteConfig {
                path: "/api/test".to_string(),
                backend: "http://localhost:3000".to_string(),
                methods: vec!["INVALID".to_string()],
                strip_prefix: false,
                description: "".to_string(),
                auth: None,
                rate_limit: None,
            }],
            auth: None,
            rate_limiting: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_config() {
        let config = GatewayConfig {
            server: ServerConfig::default(),
            routes: vec![RouteConfig {
                path: "/api/test".to_string(),
                backend: "http://localhost:3000".to_string(),
                methods: vec!["GET".to_string(), "POST".to_string()],
                strip_prefix: false,
                description: "Test route".to_string(),
                auth: None,
                rate_limit: None,
            }],
            auth: None,
            rate_limiting: None,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rate_limit_config() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 8080

routes:
  - path: "/api/test"
    backend: "http://localhost:3000"
    rate_limit:
      - dimension: ip
        requests: 100
        window_secs: 60

rate_limiting:
  enabled: true
  global:
    - dimension: ip
      requests: 1000
      window_secs: 3600
"#;

        let config = GatewayConfig::from_yaml(yaml).unwrap();
        assert!(config.rate_limiting.is_some());
        assert_eq!(config.rate_limiting.as_ref().unwrap().global.len(), 1);
        assert_eq!(config.routes[0].rate_limit.as_ref().unwrap().len(), 1);
        assert!(config.validate().is_ok());
    }
}

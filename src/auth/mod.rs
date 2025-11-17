pub mod api_key;
pub mod jwt;
pub mod middleware;

use crate::config::{AuthConfig, RouteAuthConfig};
use crate::error::{GatewayError, Result};
use axum::http::HeaderMap;
use std::sync::Arc;

/// Authentication result containing user information
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// User identifier
    pub user_id: String,
    /// Authentication method used
    pub method: AuthMethodType,
    /// Additional claims or metadata
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethodType {
    Jwt,
    ApiKey,
}

/// Authentication service that handles all authentication methods
#[derive(Clone)]
pub struct AuthService {
    jwt_validator: Option<Arc<jwt::JwtValidator>>,
    api_key_validator: Option<Arc<api_key::ApiKeyValidator>>,
}

impl AuthService {
    /// Create a new authentication service from configuration
    pub async fn new(config: Option<&AuthConfig>) -> Result<Self> {
        let config = match config {
            Some(c) => c,
            None => {
                return Ok(Self {
                    jwt_validator: None,
                    api_key_validator: None,
                })
            }
        };

        let jwt_validator = if let Some(jwt_config) = &config.jwt {
            Some(Arc::new(jwt::JwtValidator::new(jwt_config)?))
        } else {
            None
        };

        let api_key_validator = if let Some(api_key_config) = &config.api_key {
            Some(Arc::new(
                api_key::ApiKeyValidator::new(api_key_config).await?,
            ))
        } else {
            None
        };

        Ok(Self {
            jwt_validator,
            api_key_validator,
        })
    }

    /// Authenticate a request based on route configuration
    pub async fn authenticate(
        &self,
        headers: &HeaderMap,
        route_auth: &RouteAuthConfig,
    ) -> Result<AuthResult> {
        // If no methods specified, try all available methods
        let methods = if route_auth.methods.is_empty() {
            vec![]
        } else {
            route_auth.methods.clone()
        };

        let mut errors = Vec::new();

        // Try JWT authentication
        if methods.is_empty()
            || methods
                .iter()
                .any(|m| *m == crate::config::AuthMethod::Jwt)
        {
            if let Some(validator) = &self.jwt_validator {
                match validator.validate(headers).await {
                    Ok(result) => return Ok(result),
                    Err(e) => errors.push(format!("JWT: {}", e)),
                }
            }
        }

        // Try API key authentication
        if methods.is_empty()
            || methods
                .iter()
                .any(|m| *m == crate::config::AuthMethod::ApiKey)
        {
            if let Some(validator) = &self.api_key_validator {
                match validator.validate(headers).await {
                    Ok(result) => return Ok(result),
                    Err(e) => errors.push(format!("API Key: {}", e)),
                }
            }
        }

        // No authentication method succeeded
        if errors.is_empty() {
            Err(GatewayError::MissingCredentials)
        } else {
            Err(GatewayError::Unauthorized(errors.join("; ")))
        }
    }

    /// Check if authentication is available
    pub fn is_available(&self) -> bool {
        self.jwt_validator.is_some() || self.api_key_validator.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auth_service_without_config() {
        let service = AuthService::new(None).await.unwrap();
        assert!(!service.is_available());
    }
}

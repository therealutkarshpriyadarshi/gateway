use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Result type for gateway operations
pub type Result<T> = std::result::Result<T, GatewayError>;

/// Gateway error types
#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Route not found: {0}")]
    RouteNotFound(String),

    #[error("Invalid route configuration: {0}")]
    InvalidRoute(String),

    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Invalid method: {0}")]
    InvalidMethod(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Authentication failed: {0}")]
    Unauthorized(String),

    #[error("Invalid JWT token: {0}")]
    InvalidToken(String),

    #[error("Missing authentication credentials")]
    MissingCredentials,

    #[error("Invalid API key")]
    InvalidApiKey,
}

impl GatewayError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            GatewayError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::RouteNotFound(_) => StatusCode::NOT_FOUND,
            GatewayError::InvalidRoute(_) => StatusCode::BAD_REQUEST,
            GatewayError::Proxy(_) => StatusCode::BAD_GATEWAY,
            GatewayError::Backend(_) => StatusCode::BAD_GATEWAY,
            GatewayError::InvalidMethod(_) => StatusCode::METHOD_NOT_ALLOWED,
            GatewayError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            GatewayError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Http(_) => StatusCode::BAD_REQUEST,
            GatewayError::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GatewayError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            GatewayError::InvalidToken(_) => StatusCode::UNAUTHORIZED,
            GatewayError::MissingCredentials => StatusCode::UNAUTHORIZED,
            GatewayError::InvalidApiKey => StatusCode::UNAUTHORIZED,
        }
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(json!({
            "error": self.to_string(),
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            GatewayError::RouteNotFound("test".to_string()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            GatewayError::InvalidMethod("test".to_string()).status_code(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            GatewayError::Timeout("test".to_string()).status_code(),
            StatusCode::GATEWAY_TIMEOUT
        );
    }

    #[test]
    fn test_error_display() {
        let err = GatewayError::RouteNotFound("/test".to_string());
        assert_eq!(err.to_string(), "Route not found: /test");
    }
}

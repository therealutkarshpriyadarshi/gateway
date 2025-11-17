use crate::auth::AuthService;
use crate::error::{GatewayError, Result};
use crate::router::Router;
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response},
    response::IntoResponse,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Proxy handler state
#[derive(Clone)]
pub struct ProxyState {
    pub router: Arc<Router>,
    pub client: reqwest::Client,
    pub auth_service: Option<Arc<AuthService>>,
}

impl ProxyState {
    /// Create a new proxy state
    pub fn new(router: Router, timeout: Duration, auth_service: Option<AuthService>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            router: Arc::new(router),
            client,
            auth_service: auth_service.map(Arc::new),
        }
    }
}

/// Main proxy handler that forwards requests to backend services
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<impl IntoResponse> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    let query = uri.query();

    info!(
        method = %method,
        path = %path,
        "Incoming request"
    );

    // Check for health check bypass
    if is_health_check_path(path) {
        debug!("Health check path detected, bypassing authentication");
    }

    // Match the route
    let route_match = state.router.match_route(path, &method)?;

    debug!(
        backend = %route_match.route.backend,
        params = ?route_match.params,
        "Route matched"
    );

    // Perform authentication if required and not a health check
    if !is_health_check_path(path) {
        if let Some(route_auth) = &route_match.route.auth {
            if route_auth.required {
                if let Some(auth_service) = &state.auth_service {
                    let headers = req.headers();
                    match auth_service.authenticate(headers, route_auth).await {
                        Ok(auth_result) => {
                            info!(
                                user_id = %auth_result.user_id,
                                method = ?auth_result.method,
                                "Authentication successful"
                            );
                        }
                        Err(e) => {
                            warn!(error = %e, "Authentication failed");
                            return Err(e);
                        }
                    }
                } else {
                    return Err(GatewayError::Config(
                        "Authentication required but no auth service configured".to_string(),
                    ));
                }
            }
        }
    }

    // Build backend URL
    let mut backend_url = route_match.build_backend_url(path);
    if let Some(q) = query {
        backend_url.push('?');
        backend_url.push_str(q);
    }

    debug!(backend_url = %backend_url, "Forwarding to backend");

    // Forward the request
    let response = forward_request(state.client, req, &backend_url).await?;

    info!(
        status = %response.status(),
        backend = %route_match.route.backend,
        "Request completed"
    );

    Ok(response)
}

/// Forward the request to the backend service
async fn forward_request(
    client: reqwest::Client,
    req: Request<Body>,
    backend_url: &str,
) -> Result<Response<Body>> {
    let method = req.method().clone();
    let headers = req.headers().clone();

    // Collect the body
    let body_bytes = req
        .into_body()
        .collect()
        .await
        .map_err(|e| GatewayError::Proxy(format!("Failed to read request body: {}", e)))?
        .to_bytes();

    // Build the backend request
    let mut backend_req = client
        .request(method.clone(), backend_url)
        .body(body_bytes.to_vec());

    // Forward headers (excluding hop-by-hop headers)
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        // Skip hop-by-hop headers
        if !is_hop_by_hop_header(name_str) {
            backend_req = backend_req.header(name, value);
        }
    }

    // Send the request
    let backend_response = backend_req
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                GatewayError::Timeout(format!("Backend request timed out: {}", e))
            } else if e.is_connect() {
                GatewayError::Backend(format!("Failed to connect to backend: {}", e))
            } else {
                GatewayError::Proxy(format!("Backend request failed: {}", e))
            }
        })?;

    // Build response
    let status = backend_response.status();
    let mut response_builder = Response::builder().status(status);

    // Copy response headers
    for (name, value) in backend_response.headers().iter() {
        let name_str = name.as_str();
        if !is_hop_by_hop_header(name_str) {
            response_builder = response_builder.header(name, value);
        }
    }

    // Get response body
    let body_bytes = backend_response
        .bytes()
        .await
        .map_err(|e| GatewayError::Backend(format!("Failed to read backend response: {}", e)))?;

    let response = response_builder
        .body(Body::from(body_bytes))
        .map_err(|e| GatewayError::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// Check if a header is a hop-by-hop header that should not be forwarded
fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// Check if a path is a health check endpoint that should bypass authentication
fn is_health_check_path(path: &str) -> bool {
    matches!(path, "/health" | "/healthz" | "/ready" | "/readiness" | "/ping")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hop_by_hop_headers() {
        assert!(is_hop_by_hop_header("Connection"));
        assert!(is_hop_by_hop_header("connection"));
        assert!(is_hop_by_hop_header("Keep-Alive"));
        assert!(is_hop_by_hop_header("Transfer-Encoding"));
        assert!(!is_hop_by_hop_header("Content-Type"));
        assert!(!is_hop_by_hop_header("Authorization"));
    }

    #[test]
    fn test_proxy_state_creation() {
        use crate::config::RouteConfig;

        let routes = vec![RouteConfig {
            path: "/test".to_string(),
            backend: "http://localhost:3000".to_string(),
            methods: vec![],
            strip_prefix: false,
            description: "".to_string(),
            auth: None,
            rate_limit: None,
        }];

        let _router = Router::new(routes).unwrap();
        let _state = ProxyState::new(_router, Duration::from_secs(30), None);

        // State created successfully - just testing that creation doesn't panic
    }

    #[test]
    fn test_is_health_check_path() {
        assert!(is_health_check_path("/health"));
        assert!(is_health_check_path("/healthz"));
        assert!(is_health_check_path("/ready"));
        assert!(is_health_check_path("/readiness"));
        assert!(is_health_check_path("/ping"));
        assert!(!is_health_check_path("/api/users"));
        assert!(!is_health_check_path("/healthy"));
    }
}

use crate::auth::AuthService;
use crate::cache::CacheKey;
use crate::circuit_breaker::{CircuitBreakerService, RetryExecutor};
use crate::error::{GatewayError, Result};
use crate::metrics;
use crate::router::Router;
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Method, Request, Response},
    response::IntoResponse,
};
use bytes::Bytes;
use http_body_util::BodyExt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Proxy handler state
#[derive(Clone)]
pub struct ProxyState {
    pub router: Arc<Router>,
    pub client: reqwest::Client,
    pub auth_service: Option<Arc<AuthService>>,
    pub circuit_breaker: Option<Arc<CircuitBreakerService>>,
    pub retry_executor: Option<Arc<RetryExecutor>>,
}

impl ProxyState {
    /// Create a new proxy state
    pub fn new(
        router: Router,
        timeout: Duration,
        auth_service: Option<AuthService>,
        circuit_breaker: Option<CircuitBreakerService>,
        retry_executor: Option<RetryExecutor>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            router: Arc::new(router),
            client,
            auth_service: auth_service.map(Arc::new),
            circuit_breaker: circuit_breaker.map(Arc::new),
            retry_executor: retry_executor.map(Arc::new),
        }
    }
}

/// Main proxy handler that forwards requests to backend services
#[axum::debug_handler]
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    req: Request<Body>,
) -> Result<impl IntoResponse> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    let query = uri.query();
    let client_ip = connect_info
        .map(|ConnectInfo(addr)| addr.ip())
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap());

    // Start metrics timer
    let mut timer = metrics::Timer::new(method.to_string(), path.to_string());

    info!(
        method = %method,
        path = %path,
        client_ip = %client_ip,
        "Incoming request"
    );

    // Check for health check bypass
    if is_health_check_path(path) {
        debug!("Health check path detected, bypassing authentication");
    }

    // Match the route
    let route_match = state.router.match_route(path, &method)?;

    debug!(
        params = ?route_match.params,
        "Route matched"
    );

    // Check IP filtering if configured
    if let Some(ip_filter) = &route_match.route.ip_filter {
        if !ip_filter.is_allowed(&client_ip) {
            warn!(ip = %client_ip, "IP address blocked by filter");
            timer.record(403);
            return Err(GatewayError::Forbidden(format!(
                "Access denied for IP: {}",
                client_ip
            )));
        }
        debug!(ip = %client_ip, "IP address allowed by filter");
    }

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
                            // Record successful authentication
                            metrics::record_auth_attempt(
                                &format!("{:?}", auth_result.method),
                                true,
                            );
                        }
                        Err(e) => {
                            warn!(error = %e, "Authentication failed");
                            // Record failed authentication
                            metrics::record_auth_attempt("unknown", false);
                            timer.record(401);
                            return Err(e);
                        }
                    }
                } else {
                    timer.record(500);
                    return Err(GatewayError::Config(
                        "Authentication required but no auth service configured".to_string(),
                    ));
                }
            }
        }
    }

    // Check cache if configured
    let request_headers = req.headers().clone();  // Clone headers for cache key before consuming req
    if let Some(cache) = &route_match.route.cache {
        let cache_key = CacheKey::new(
            method.to_string(),
            path.to_string(),
            query.map(|q| q.to_string()),
            &request_headers,
            cache.key_headers(),
        );

        if let Some(cached_response) = cache.get(&cache_key).await {
            debug!(
                method = %method,
                path = %path,
                "Returning cached response"
            );
            timer.record(cached_response.status.as_u16());
            return Ok(cached_response.to_response());
        }
    }

    // Select backend using load balancer
    let backend = match route_match
        .route
        .load_balancer
        .select_backend(Some(client_ip))
    {
        Some(backend) => backend,
        None => {
            timer.record(503);
            return Err(GatewayError::Backend(
                "No healthy backend available".to_string(),
            ));
        }
    };

    // Set backend on timer for metrics
    timer.set_backend(backend.url().to_string());

    debug!(
        backend = %backend.url(),
        healthy_count = route_match.route.load_balancer.healthy_count(),
        total_count = route_match.route.load_balancer.total_count(),
        "Selected backend"
    );

    // Record backend health
    metrics::record_backend_health(backend.url(), backend.is_healthy());

    // Apply path transformation if configured
    let transformed_path = if let Some(transform) = &route_match.route.transform {
        transform.transform_path(path)
    } else {
        path.to_string()
    };

    // Apply query parameter transformation if configured
    let transformed_query = if let Some(transform) = &route_match.route.transform {
        if let Some(q) = query {
            Some(transform.transform_query_params(q))
        } else {
            None
        }
    } else {
        query.map(|q| q.to_string())
    };

    // Build backend URL with transformations
    let mut backend_url = route_match.build_backend_url(backend.url(), &transformed_path);
    if let Some(q) = transformed_query.as_ref() {
        backend_url.push('?');
        backend_url.push_str(q);
    }

    debug!(backend_url = %backend_url, "Forwarding to backend");

    let backend_url_for_cb = backend.url().to_string();

    // Check circuit breaker
    if let Some(circuit_breaker) = &state.circuit_breaker {
        if !circuit_breaker.can_proceed(&backend_url_for_cb).await {
            warn!(backend = %backend_url_for_cb, "Circuit breaker open, rejecting request");
            // Record circuit breaker state as open
            metrics::record_circuit_breaker_state(&backend_url_for_cb, 1);
            timer.record(503);
            return Err(GatewayError::CircuitBreakerOpen(format!(
                "Circuit breaker is open for backend: {}",
                backend_url_for_cb
            )));
        }
    }

    // Track connection for least connections strategy
    backend.increment_connections();

    // Record active connections
    metrics::record_active_connections(backend.url(), backend.active_connections() as i64);

    // Collect request body and headers for potential retries
    let method_for_request = req.method().clone();
    let mut headers_for_request = req.headers().clone();

    // Apply request header transformations if configured
    if let Some(transform) = &route_match.route.transform {
        transform.transform_request_headers(&mut headers_for_request)?;
    }

    let body_bytes = req
        .into_body()
        .collect()
        .await
        .map_err(|e| GatewayError::Proxy(format!("Failed to read request body: {}", e)))?
        .to_bytes();

    // Forward the request with retry logic if configured
    let response: Result<Response<Body>> = if let Some(retry_executor) = &state.retry_executor {
        let client = state.client.clone();
        let backend_url_clone = backend_url.clone();
        let method_clone = method_for_request.clone();
        let headers_clone = headers_for_request.clone();
        let body_clone = body_bytes.clone();

        retry_executor
            .execute_with_predicate(
                || {
                    let client = client.clone();
                    let backend_url = backend_url_clone.clone();
                    let method = method_clone.clone();
                    let headers = headers_clone.clone();
                    let body = body_clone.clone();
                    async move { send_request(client, method, headers, body, &backend_url).await }
                },
                |e| {
                    // Only retry on timeout or connection errors
                    matches!(e, GatewayError::Timeout(_) | GatewayError::Backend(_))
                },
            )
            .await
    } else {
        send_request(
            state.client.clone(),
            method_for_request,
            headers_for_request,
            body_bytes,
            &backend_url,
        )
        .await
    };

    // Decrement connection counter
    backend.decrement_connections();

    // Record active connections after decrement
    metrics::record_active_connections(backend.url(), backend.active_connections() as i64);

    // Record result in circuit breaker
    if let Some(circuit_breaker) = &state.circuit_breaker {
        match &response {
            Ok(resp) => {
                // Consider 5xx status codes as failures
                if resp.status().is_server_error() {
                    circuit_breaker.record_failure(&backend_url_for_cb).await;
                    metrics::record_circuit_breaker_state(&backend_url_for_cb, 0);
                } else {
                    circuit_breaker.record_success(&backend_url_for_cb).await;
                    metrics::record_circuit_breaker_state(&backend_url_for_cb, 0);
                }
            }
            Err(e) => match e {
                GatewayError::Timeout(_) => {
                    circuit_breaker.record_timeout(&backend_url_for_cb).await;
                    metrics::record_circuit_breaker_state(&backend_url_for_cb, 0);
                }
                _ => {
                    circuit_breaker.record_failure(&backend_url_for_cb).await;
                    metrics::record_circuit_breaker_state(&backend_url_for_cb, 0);
                }
            },
        }
    }

    // Passive health check
    if let Some(health_checker) = &route_match.route.health_checker {
        let success = response.is_ok() && !response.as_ref().unwrap().status().is_server_error();
        health_checker.passive_check(&backend, success);

        // Update backend health metric
        metrics::record_backend_health(backend.url(), backend.is_healthy());
    }

    // Record final metrics and log result
    let final_status = match &response {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            info!(
                status = %resp.status(),
                backend = %backend.url(),
                latency_ms = timer.elapsed() * 1000.0,
                "Request completed"
            );
            status_code
        }
        Err(e) => {
            let status_code = match e {
                GatewayError::Unauthorized(_) | GatewayError::InvalidToken(_) => 401,
                GatewayError::RouteNotFound(_) => 404,
                GatewayError::InvalidMethod(_) => 405,
                GatewayError::Timeout(_) => 504,
                GatewayError::CircuitBreakerOpen(_) => 503,
                _ => 502,
            };
            warn!(
                error = %e,
                backend = %backend.url(),
                latency_ms = timer.elapsed() * 1000.0,
                "Request failed"
            );
            status_code
        }
    };

    // Record metrics with timer
    timer.record(final_status);

    // Apply response transformations and caching if successful
    let mut final_response = response?;

    // Apply response header transformations if configured
    if let Some(transform) = &route_match.route.transform {
        let headers = final_response.headers_mut();
        transform.transform_response_headers(headers)?;
    }

    // Store in cache if configured and response is cacheable
    if let Some(cache) = &route_match.route.cache {
        // Create cache key using original request headers
        let cache_key = CacheKey::new(
            method.to_string(),
            path.to_string(),
            query.map(|q| q.to_string()),
            &request_headers,  // Use original request headers for cache key
            cache.key_headers(),
        );

        // Extract response parts for caching
        let (parts, body) = final_response.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| GatewayError::Backend(format!("Failed to read response body: {}", e)))?
            .to_bytes();

        // Store in cache
        cache
            .put(
                cache_key,
                parts.status,
                parts.headers.clone(),
                body_bytes.clone(),
            )
            .await?;

        // Reconstruct response
        final_response = Response::from_parts(parts, Body::from(body_bytes));
    }

    Ok(final_response)
}

/// Send request to the backend service
async fn send_request(
    client: reqwest::Client,
    method: Method,
    headers: HeaderMap,
    body_bytes: Bytes,
    backend_url: &str,
) -> Result<Response<Body>> {
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
    let backend_response = backend_req.send().await.map_err(|e| {
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
    matches!(
        path,
        "/health" | "/healthz" | "/ready" | "/readiness" | "/ping"
    )
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
            backend: Some("http://localhost:3000".to_string()),
            backends: vec![],
            load_balancer: None,
            health_check: None,
            methods: vec![],
            strip_prefix: false,
            description: "".to_string(),
            auth: None,
            rate_limit: None,
            transform: None,
            cors: None,
            ip_filter: None,
            cache: None,
        }];

        let _router = Router::new(routes).unwrap();
        let _state = ProxyState::new(_router, Duration::from_secs(30), None, None, None);

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

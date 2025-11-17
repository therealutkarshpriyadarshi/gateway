use crate::error::{GatewayError, Result};
use axum::{
    body::Body,
    extract::State,
    http::{Response, StatusCode},
    response::IntoResponse,
};
use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

/// Metrics service for collecting and exposing Prometheus metrics
#[derive(Clone)]
pub struct MetricsService {
    handle: Arc<PrometheusHandle>,
}

impl MetricsService {
    /// Create a new metrics service
    pub fn new() -> Result<Self> {
        let handle = PrometheusBuilder::new().install_recorder().map_err(|e| {
            GatewayError::Internal(format!("Failed to install metrics recorder: {}", e))
        })?;

        // Register all metrics with descriptions
        Self::register_metrics();

        info!("Metrics service initialized successfully");

        Ok(Self {
            handle: Arc::new(handle),
        })
    }

    /// Register all metrics with descriptions
    fn register_metrics() {
        // Request metrics
        describe_counter!(
            "gateway_requests_total",
            "Total number of HTTP requests received"
        );
        describe_histogram!(
            "gateway_request_duration_seconds",
            "HTTP request latencies in seconds"
        );
        describe_counter!(
            "gateway_requests_errors_total",
            "Total number of HTTP requests that resulted in errors"
        );

        // Backend metrics
        describe_counter!(
            "gateway_backend_requests_total",
            "Total number of requests sent to backends"
        );
        describe_counter!(
            "gateway_backend_errors_total",
            "Total number of backend errors"
        );
        describe_histogram!(
            "gateway_backend_duration_seconds",
            "Backend request latencies in seconds"
        );
        describe_gauge!(
            "gateway_backend_healthy",
            "Backend health status (1 = healthy, 0 = unhealthy)"
        );

        // Circuit breaker metrics
        describe_gauge!(
            "gateway_circuit_breaker_state",
            "Circuit breaker state (0 = closed, 1 = open, 2 = half-open)"
        );
        describe_counter!(
            "gateway_circuit_breaker_transitions_total",
            "Total number of circuit breaker state transitions"
        );

        // Connection metrics
        describe_gauge!(
            "gateway_active_connections",
            "Number of active connections to backends"
        );

        // Authentication metrics
        describe_counter!(
            "gateway_auth_attempts_total",
            "Total number of authentication attempts"
        );
        describe_counter!(
            "gateway_auth_failures_total",
            "Total number of authentication failures"
        );

        // Rate limiting metrics
        describe_counter!(
            "gateway_rate_limit_exceeded_total",
            "Total number of requests rejected due to rate limiting"
        );

        debug!("All metrics registered with descriptions");
    }

    /// Get the Prometheus metrics handle
    pub fn handle(&self) -> Arc<PrometheusHandle> {
        self.handle.clone()
    }

    /// Render metrics in Prometheus format
    pub fn render(&self) -> String {
        self.handle.render()
    }
}

/// Metrics endpoint handler
pub async fn metrics_handler(State(service): State<MetricsService>) -> impl IntoResponse {
    let metrics = service.render();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(Body::from(metrics))
        .unwrap()
}

/// Record a request metric
pub fn record_request(method: &str, path: &str, status: u16, duration: f64) {
    let labels = [
        ("method", method.to_string()),
        ("path", sanitize_path(path)),
        ("status", status.to_string()),
    ];

    counter!("gateway_requests_total", &labels).increment(1);
    histogram!("gateway_request_duration_seconds", &labels).record(duration);

    if status >= 400 {
        counter!("gateway_requests_errors_total", &labels).increment(1);
    }
}

/// Record a backend request metric
pub fn record_backend_request(backend: &str, method: &str, status: u16, duration: f64) {
    let labels = [
        ("backend", backend.to_string()),
        ("method", method.to_string()),
        ("status", status.to_string()),
    ];

    counter!("gateway_backend_requests_total", &labels).increment(1);
    histogram!("gateway_backend_duration_seconds", &labels).record(duration);

    if status >= 400 {
        counter!("gateway_backend_errors_total", &labels).increment(1);
    }
}

/// Record backend health status
pub fn record_backend_health(backend: &str, healthy: bool) {
    let labels = [("backend", backend.to_string())];
    gauge!("gateway_backend_healthy", &labels).set(if healthy { 1.0 } else { 0.0 });
}

/// Record circuit breaker state
/// State: 0 = Closed, 1 = Open, 2 = HalfOpen
pub fn record_circuit_breaker_state(backend: &str, state: u8) {
    let labels = [("backend", backend.to_string())];
    gauge!("gateway_circuit_breaker_state", &labels).set(state as f64);
}

/// Record circuit breaker transition
pub fn record_circuit_breaker_transition(backend: &str, from_state: &str, to_state: &str) {
    let labels = [
        ("backend", backend.to_string()),
        ("from", from_state.to_string()),
        ("to", to_state.to_string()),
    ];
    counter!("gateway_circuit_breaker_transitions_total", &labels).increment(1);
}

/// Record active connections
pub fn record_active_connections(backend: &str, count: i64) {
    let labels = [("backend", backend.to_string())];
    gauge!("gateway_active_connections", &labels).set(count as f64);
}

/// Record authentication attempt
pub fn record_auth_attempt(method: &str, success: bool) {
    let labels = [("method", method.to_string())];
    counter!("gateway_auth_attempts_total", &labels).increment(1);

    if !success {
        counter!("gateway_auth_failures_total", &labels).increment(1);
    }
}

/// Record rate limit exceeded
pub fn record_rate_limit_exceeded(identifier: &str, route: &str) {
    let labels = [
        ("identifier", identifier.to_string()),
        ("route", route.to_string()),
    ];
    counter!("gateway_rate_limit_exceeded_total", &labels).increment(1);
}

/// Sanitize path for metrics to avoid cardinality explosion
/// Replaces path parameters with placeholders
fn sanitize_path(path: &str) -> String {
    // Split path into segments
    let segments: Vec<&str> = path.split('/').collect();

    // Replace segments that look like IDs/UUIDs with placeholders
    let sanitized: Vec<String> = segments
        .iter()
        .map(|seg| {
            if seg.is_empty() {
                String::new()
            } else if is_likely_id(seg) {
                ":id".to_string()
            } else {
                (*seg).to_string()
            }
        })
        .collect();

    sanitized.join("/")
}

/// Check if a path segment is likely an ID (numeric, UUID, etc.)
fn is_likely_id(segment: &str) -> bool {
    // Check if all numeric
    if segment.chars().all(|c| c.is_numeric()) {
        return true;
    }

    // Check if looks like a UUID (contains hyphens and hex chars)
    if segment.len() >= 32 && segment.contains('-') {
        return segment.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    }

    // Check if looks like a hash (alphanumeric string > 10 chars)
    // This catches IDs like "abc123def456", MongoDB ObjectIds, etc.
    if segment.len() > 10 && segment.chars().all(|c| c.is_alphanumeric()) {
        // Make sure it has at least some numbers (to avoid catching normal words)
        let has_numbers = segment.chars().any(|c| c.is_numeric());
        let has_letters = segment.chars().any(|c| c.is_alphabetic());
        if has_numbers && has_letters {
            return true;
        }
    }

    false
}

/// Timer for measuring request duration
pub struct Timer {
    start: Instant,
    method: String,
    path: String,
    backend: Option<String>,
}

impl Timer {
    /// Start a new timer for a request
    pub fn new(method: String, path: String) -> Self {
        Self {
            start: Instant::now(),
            method,
            path,
            backend: None,
        }
    }

    /// Set the backend for this timer
    pub fn set_backend(&mut self, backend: String) {
        self.backend = Some(backend);
    }

    /// Record the elapsed time with the given status code
    pub fn record(self, status: u16) {
        let duration = self.start.elapsed().as_secs_f64();
        record_request(&self.method, &self.path, status, duration);

        if let Some(backend) = &self.backend {
            record_backend_request(backend, &self.method, status, duration);
        }
    }

    /// Get the elapsed time in seconds
    pub fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        assert_eq!(sanitize_path("/api/users/123"), "/api/users/:id");
        assert_eq!(sanitize_path("/api/users/abc123def456"), "/api/users/:id");
        assert_eq!(
            sanitize_path("/api/users/550e8400-e29b-41d4-a716-446655440000"),
            "/api/users/:id"
        );
        assert_eq!(sanitize_path("/api/users"), "/api/users");
        assert_eq!(sanitize_path("/api/users/profile"), "/api/users/profile");
    }

    #[test]
    fn test_is_likely_id() {
        assert!(is_likely_id("123"));
        assert!(is_likely_id("123456789"));
        assert!(is_likely_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(is_likely_id("abc123def456ghi789jkl012"));
        assert!(!is_likely_id("users"));
        assert!(!is_likely_id("profile"));
        assert!(!is_likely_id("api"));
    }

    #[test]
    fn test_timer_creation() {
        let timer = Timer::new("GET".to_string(), "/api/users".to_string());
        assert_eq!(timer.method, "GET");
        assert_eq!(timer.path, "/api/users");
        assert!(timer.backend.is_none());
        assert!(timer.elapsed() >= 0.0);
    }

    #[test]
    fn test_timer_with_backend() {
        let mut timer = Timer::new("POST".to_string(), "/api/data".to_string());
        timer.set_backend("http://backend:3000".to_string());
        assert_eq!(timer.backend, Some("http://backend:3000".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_service_creation() {
        // This test may fail if metrics recorder is already installed
        // In a real scenario, this would be called once at startup
        let result = MetricsService::new();

        // We don't assert Ok() here because the recorder might already be installed
        // in other tests. The important thing is that the function doesn't panic.
        match result {
            Ok(_service) => {
                // Service created successfully
            }
            Err(e) => {
                // Expected if recorder already installed
                assert!(e.to_string().contains("recorder") || e.to_string().contains("install"));
            }
        }
    }

    #[test]
    fn test_record_functions_dont_panic() {
        // These functions should not panic even if recorder isn't installed
        record_request("GET", "/api/test", 200, 0.123);
        record_backend_request("http://backend:3000", "POST", 201, 0.456);
        record_backend_health("http://backend:3000", true);
        record_circuit_breaker_state("http://backend:3000", 0);
        record_circuit_breaker_transition("http://backend:3000", "closed", "open");
        record_active_connections("http://backend:3000", 5);
        record_auth_attempt("jwt", true);
        record_rate_limit_exceeded("127.0.0.1", "/api/test");
    }
}

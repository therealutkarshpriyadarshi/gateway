use axum::{body::Body, extract::Request, http::HeaderMap, middleware::Next, response::Response};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    trace::{self, RandomIdGenerator, Sampler},
    Resource,
};
use std::time::SystemTime;
use tracing::{info, Span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

use crate::error::{GatewayError, Result};

pub const REQUEST_ID_HEADER: &str = "x-request-id";
pub const TRACE_ID_HEADER: &str = "x-trace-id";

/// Configuration for OpenTelemetry tracing
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// OTLP endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: String,
    /// Service name for traces
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Sample rate (0.0 to 1.0)
    pub sample_rate: f64,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: "http://localhost:4317".to_string(),
            service_name: "api-gateway".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            sample_rate: 1.0,
        }
    }
}

/// Initialize OpenTelemetry tracing and return the tracer
pub fn init_telemetry(config: TracingConfig) -> Result<opentelemetry_sdk::trace::Tracer> {
    info!(
        endpoint = %config.otlp_endpoint,
        service = %config.service_name,
        version = %config.service_version,
        "Initializing OpenTelemetry tracing"
    );

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&config.otlp_endpoint);

    // Create and install tracer (install_batch returns a Tracer directly)
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            trace::config()
                .with_sampler(Sampler::TraceIdRatioBased(config.sample_rate))
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(Resource::new(vec![
                    KeyValue::new("service.name", config.service_name.clone()),
                    KeyValue::new("service.version", config.service_version.clone()),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .map_err(|e| GatewayError::Internal(format!("Failed to install tracer: {}", e)))?;

    info!("OpenTelemetry tracing initialized successfully");

    Ok(tracer)
}

/// Initialize tracing with optional OpenTelemetry support
pub fn init_tracing(otel_config: Option<TracingConfig>) -> Result<()> {
    if let Some(config) = otel_config {
        // Initialize with OpenTelemetry
        let tracer = init_telemetry(config)?;
        let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "gateway=debug,tower_http=debug".into());

        tracing_subscriber::registry()
            .with(telemetry_layer)
            .with(env_filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .compact(),
            )
            .init();

        info!("Tracing initialized with OpenTelemetry support");
    } else {
        // Initialize without OpenTelemetry (basic tracing)
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "gateway=debug,tower_http=debug".into());

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .compact()
            .init();

        info!("Tracing initialized without OpenTelemetry");
    }

    Ok(())
}

/// Shutdown OpenTelemetry gracefully
pub fn shutdown_telemetry() {
    info!("Shutting down OpenTelemetry");
    global::shutdown_tracer_provider();
}

/// Middleware to add request ID to requests
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    // Check if request already has a request ID
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Add request ID to tracing span
    Span::current().record("request_id", &request_id);

    // Store request ID in request extensions for later use
    req.extensions_mut().insert(RequestId(request_id.clone()));

    // Process the request
    let mut response = next.run(req).await;

    // Add request ID to response headers
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id
            .parse()
            .unwrap_or_else(|_| "invalid".parse().unwrap()),
    );

    response
}

/// Middleware to add trace context propagation
pub async fn trace_context_middleware(req: Request, next: Next) -> Response {
    use opentelemetry::trace::{SpanKind, TraceContextExt, Tracer};
    use opentelemetry::Context;

    // Extract trace context from headers if present
    let parent_context = extract_trace_context(req.headers());

    // Create a new span
    let tracer = global::tracer("gateway");
    let span = tracer
        .span_builder(format!("{} {}", req.method(), req.uri().path()))
        .with_kind(SpanKind::Server)
        .with_start_time(SystemTime::now())
        .start_with_context(&tracer, &parent_context);

    let cx = Context::current_with_span(span);

    // Attach context to current scope
    let _guard = cx.attach();

    // Process request
    let response = next.run(req).await;

    response
}

/// Extract trace context from headers (W3C Trace Context format)
fn extract_trace_context(headers: &HeaderMap) -> opentelemetry::Context {
    use opentelemetry::propagation::TextMapPropagator;
    use opentelemetry_sdk::propagation::TraceContextPropagator;

    let propagator = TraceContextPropagator::new();
    let context = propagator.extract(&HeaderMapCarrier(headers));

    context
}

/// Carrier for extracting trace context from HeaderMap
struct HeaderMapCarrier<'a>(&'a HeaderMap);

impl<'a> opentelemetry::propagation::Extractor for HeaderMapCarrier<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

/// Request ID extension type
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Extract request ID from request extensions
pub fn get_request_id(req: &Request<Body>) -> Option<String> {
    req.extensions().get::<RequestId>().map(|id| id.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "api-gateway");
        assert_eq!(config.sample_rate, 1.0);
    }

    #[test]
    fn test_request_id_extraction() {
        let mut headers = HeaderMap::new();
        headers.insert(
            REQUEST_ID_HEADER,
            HeaderValue::from_static("test-request-id"),
        );

        let request_id = headers
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        assert_eq!(request_id, Some("test-request-id".to_string()));
    }

    #[test]
    fn test_uuid_generation() {
        let uuid1 = Uuid::new_v4().to_string();
        let uuid2 = Uuid::new_v4().to_string();

        // UUIDs should be valid and different
        assert_ne!(uuid1, uuid2);
        assert_eq!(uuid1.len(), 36); // Standard UUID length
    }

    #[test]
    fn test_request_id_type() {
        let request_id = RequestId("test-id".to_string());
        assert_eq!(request_id.0, "test-id");
    }

    #[test]
    fn test_header_map_carrier() {
        use axum::http::HeaderValue;
        use opentelemetry::propagation::Extractor;

        let mut headers = HeaderMap::new();
        headers.insert("traceparent", HeaderValue::from_static("test-trace-parent"));
        headers.insert("tracestate", HeaderValue::from_static("test-trace-state"));

        let carrier = HeaderMapCarrier(&headers);
        assert_eq!(carrier.get("traceparent"), Some("test-trace-parent"));
        assert_eq!(carrier.get("tracestate"), Some("test-trace-state"));
        assert_eq!(carrier.get("nonexistent"), None);

        let keys = carrier.keys();
        assert!(keys.contains(&"traceparent"));
        assert!(keys.contains(&"tracestate"));
    }
}

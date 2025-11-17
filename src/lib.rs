pub mod auth;
pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod healthcheck;
pub mod loadbalancer;
pub mod metrics;
pub mod observability;
pub mod proxy;
pub mod rate_limit;
pub mod router;

use crate::config::GatewayConfig;
use crate::error::Result;
use crate::metrics::{metrics_handler, MetricsService};
use crate::observability::{request_id_middleware, TracingConfig};
use crate::proxy::{proxy_handler, ProxyState};
use crate::router::Router;
use axum::{middleware, routing::any, routing::get, Router as AxumRouter};
use std::time::Duration;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Initialize the gateway server
pub async fn init_gateway(config: GatewayConfig) -> Result<()> {
    // Validate configuration
    config.validate()?;

    info!("Starting API Gateway");
    info!(
        "Server listening on {}:{}",
        config.server.host, config.server.port
    );

    // Create authentication service if configured
    let auth_service = if config.auth.is_some() {
        info!("Initializing authentication service");
        let service = auth::AuthService::new(config.auth.as_ref()).await?;
        if service.is_available() {
            info!("Authentication service initialized successfully");
            Some(service)
        } else {
            info!("No authentication methods configured");
            None
        }
    } else {
        info!("Authentication not configured");
        None
    };

    // Create circuit breaker service if configured
    let circuit_breaker = if let Some(cb_config) = config.circuit_breaker {
        info!(
            failure_threshold = cb_config.failure_threshold,
            success_threshold = cb_config.success_threshold,
            timeout_secs = cb_config.timeout_secs,
            "Initializing circuit breaker service"
        );
        Some(circuit_breaker::CircuitBreakerService::new(cb_config))
    } else {
        info!("Circuit breaker not configured");
        None
    };

    // Create retry executor if configured
    let retry_executor = if let Some(retry_config) = config.retry {
        info!(
            max_retries = retry_config.max_retries,
            initial_backoff_ms = retry_config.initial_backoff_ms,
            "Initializing retry executor"
        );
        Some(circuit_breaker::RetryExecutor::new(retry_config))
    } else {
        info!("Retry logic not configured");
        None
    };

    // Create router
    let router = Router::new(config.routes)?;
    info!("Loaded {} routes", router.routes().len());

    // Create proxy state
    let proxy_state = ProxyState::new(
        router,
        Duration::from_secs(config.server.timeout_secs),
        auth_service,
        circuit_breaker,
        retry_executor,
    );

    // Initialize metrics service if configured
    let metrics_service = if let Some(obs_config) = &config.observability {
        if let Some(metrics_config) = &obs_config.metrics {
            if metrics_config.enabled {
                info!("Initializing Prometheus metrics service");
                let service = MetricsService::new()?;
                info!("Metrics endpoint enabled at {}", metrics_config.path);
                Some((service, metrics_config.path.clone()))
            } else {
                info!("Metrics disabled in configuration");
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Create Axum app
    let mut app = AxumRouter::new()
        .route("/*path", any(proxy_handler))
        .with_state(proxy_state);

    // Add metrics endpoint if configured
    if let Some((metrics_service, metrics_path)) = metrics_service {
        app = app.route(
            &metrics_path,
            get(metrics_handler).with_state(metrics_service),
        );
    }

    // Add middleware layers
    app = app
        .layer(middleware::from_fn(request_id_middleware))
        .layer(TraceLayer::new_for_http());

    // Bind and serve
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(crate::error::GatewayError::Io)?;

    info!("Gateway ready to accept connections");

    // Use make_service_with_connect_info to extract client IP
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .map_err(|e| crate::error::GatewayError::Internal(format!("Server error: {}", e)))?;

    Ok(())
}

/// Initialize tracing/logging with optional OpenTelemetry support
pub fn init_tracing(config: Option<&GatewayConfig>) -> Result<()> {
    // Check if OpenTelemetry is configured
    let otel_config = config
        .and_then(|c| c.observability.as_ref())
        .and_then(|o| o.tracing.as_ref())
        .filter(|t| t.enabled)
        .map(|t| TracingConfig {
            otlp_endpoint: t.otlp_endpoint.clone(),
            service_name: t.service_name.clone(),
            service_version: t.service_version.clone(),
            sample_rate: t.sample_rate,
        });

    observability::init_tracing(otel_config)
}

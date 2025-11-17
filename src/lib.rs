pub mod auth;
pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod healthcheck;
pub mod loadbalancer;
pub mod proxy;
pub mod rate_limit;
pub mod router;

use crate::config::GatewayConfig;
use crate::error::Result;
use crate::proxy::{proxy_handler, ProxyState};
use crate::router::Router;
use axum::{routing::any, Router as AxumRouter};
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

    // Create Axum app
    let app = AxumRouter::new()
        .route("/*path", any(proxy_handler))
        .with_state(proxy_state)
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

/// Initialize tracing/logging
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gateway=debug,tower_http=debug".into()),
        )
        .with_target(false)
        .compact()
        .init();
}

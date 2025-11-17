pub mod config;
pub mod error;
pub mod proxy;
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

    // Create router
    let router = Router::new(config.routes)?;
    info!("Loaded {} routes", router.routes().len());

    // Create proxy state
    let proxy_state = ProxyState::new(router, Duration::from_secs(config.server.timeout_secs));

    // Create Axum app
    let app = AxumRouter::new()
        .route("/*path", any(proxy_handler))
        .with_state(proxy_state)
        .layer(TraceLayer::new_for_http());

    // Bind and serve
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| crate::error::GatewayError::Io(e))?;

    info!("Gateway ready to accept connections");

    axum::serve(listener, app)
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

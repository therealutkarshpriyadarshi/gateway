use gateway::{config::GatewayConfig, init_gateway, init_tracing};
use std::env;
use std::process;

#[tokio::main]
async fn main() {
    // Initialize tracing
    init_tracing();

    // Get config file path from command line or use default
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "config/gateway.yaml".to_string());

    // Load configuration
    let config = match GatewayConfig::from_file(&config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration from {}: {}", config_path, e);
            eprintln!("Usage: gateway [config_file]");
            process::exit(1);
        }
    };

    // Start the gateway
    if let Err(e) = init_gateway(config).await {
        eprintln!("Gateway error: {}", e);
        process::exit(1);
    }
}

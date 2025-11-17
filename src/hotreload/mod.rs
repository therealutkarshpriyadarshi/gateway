use crate::config::GatewayConfig;
use crate::error::{GatewayError, Result};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

/// Hot reload configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct HotReloadConfig {
    /// Enable hot reload
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Debounce delay in milliseconds (to avoid reloading multiple times for rapid changes)
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_enabled() -> bool {
    false
}

fn default_debounce_ms() -> u64 {
    1000 // 1 second
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            debounce_ms: default_debounce_ms(),
        }
    }
}

/// Shared configuration that can be updated via hot reload
#[derive(Clone)]
pub struct ReloadableConfig {
    inner: Arc<RwLock<GatewayConfig>>,
}

impl ReloadableConfig {
    /// Create a new reloadable configuration
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(config)),
        }
    }

    /// Get a read lock on the configuration
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, GatewayConfig> {
        self.inner.read().await
    }

    /// Update the configuration
    pub async fn update(&self, new_config: GatewayConfig) -> Result<()> {
        // Validate new configuration before applying
        new_config.validate()?;

        let mut config = self.inner.write().await;
        *config = new_config;

        info!("Configuration updated via hot reload");
        Ok(())
    }
}

/// Hot reload service that watches for configuration file changes
pub struct HotReloadService {
    config_path: PathBuf,
    reloadable_config: ReloadableConfig,
    debounce_duration: Duration,
}

impl HotReloadService {
    /// Create a new hot reload service
    pub fn new(
        config_path: PathBuf,
        reloadable_config: ReloadableConfig,
        debounce_ms: u64,
    ) -> Self {
        Self {
            config_path,
            reloadable_config,
            debounce_duration: Duration::from_millis(debounce_ms),
        }
    }

    /// Start watching the configuration file for changes
    pub async fn start(self) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(100);

        // Create a watcher
        let mut watcher: RecommendedWatcher = Watcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    // Only care about modify events
                    if matches!(
                        event.kind,
                        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                    ) {
                        let _ = tx.blocking_send(event);
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| GatewayError::Internal(format!("Failed to create file watcher: {}", e)))?;

        // Watch the config file
        watcher
            .watch(&self.config_path, RecursiveMode::NonRecursive)
            .map_err(|e| {
                GatewayError::Internal(format!("Failed to watch config file: {}", e))
            })?;

        info!(
            path = %self.config_path.display(),
            debounce_ms = self.debounce_duration.as_millis(),
            "Hot reload watcher started"
        );

        // Spawn a task to handle file change events
        tokio::spawn(async move {
            let mut last_reload = std::time::Instant::now();

            while let Some(event) = rx.recv().await {
                debug!("File change event detected: {:?}", event);

                // Debounce: ignore events that are too close together
                let now = std::time::Instant::now();
                if now.duration_since(last_reload) < self.debounce_duration {
                    debug!("Ignoring event due to debounce");
                    continue;
                }

                last_reload = now;

                // Attempt to reload configuration
                match self.reload_config().await {
                    Ok(()) => {
                        info!("Configuration reloaded successfully");
                    }
                    Err(e) => {
                        error!("Failed to reload configuration: {}", e);
                    }
                }
            }

            // Keep watcher alive
            drop(watcher);
        });

        Ok(())
    }

    /// Reload configuration from file
    async fn reload_config(&self) -> Result<()> {
        info!("Reloading configuration from {:?}", self.config_path);

        // Load new configuration
        let new_config = GatewayConfig::from_file(&self.config_path)?;

        // Validate before applying
        new_config.validate()?;

        // Update the shared configuration
        self.reloadable_config.update(new_config).await?;

        Ok(())
    }
}

/// Watch a configuration file and reload on changes
pub async fn watch_config_file<P: AsRef<Path>>(
    config_path: P,
    reloadable_config: ReloadableConfig,
    debounce_ms: u64,
) -> Result<()> {
    let service = HotReloadService::new(
        config_path.as_ref().to_path_buf(),
        reloadable_config,
        debounce_ms,
    );

    service.start().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_reloadable_config_creation() {
        let config = GatewayConfig::default_config();
        let reloadable = ReloadableConfig::new(config);
        // Just test that creation works
        drop(reloadable);
    }

    #[tokio::test]
    async fn test_reloadable_config_update() {
        let config = GatewayConfig::default_config();
        let reloadable = ReloadableConfig::new(config);

        let mut new_config = GatewayConfig::default_config();
        new_config.server.port = 9090;

        let result = reloadable.update(new_config).await;
        assert!(result.is_ok());

        let read_config = reloadable.read().await;
        assert_eq!(read_config.server.port, 9090);
    }

    #[tokio::test]
    async fn test_reloadable_config_invalid_update() {
        let config = GatewayConfig::default_config();
        let reloadable = ReloadableConfig::new(config);

        // Create invalid config (empty route path)
        let mut new_config = GatewayConfig::default_config();
        new_config.routes.push(crate::config::RouteConfig {
            path: "".to_string(), // Invalid
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
        });

        let result = reloadable.update(new_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_hot_reload_config_file() {
        // Create a temporary config file
        let temp_file = NamedTempFile::new().unwrap();
        let config_path = temp_file.path().to_path_buf();

        // Write initial config
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 8080
  timeout_secs: 30
routes: []
"#;
        fs::write(&config_path, yaml).unwrap();

        // Load initial config
        let initial_config = GatewayConfig::from_file(&config_path).unwrap();
        let reloadable = ReloadableConfig::new(initial_config);

        // Verify initial config
        {
            let config = reloadable.read().await;
            assert_eq!(config.server.port, 8080);
        }

        // Note: Full integration test of file watching would require
        // actually modifying the file and waiting for the watcher to detect it.
        // This is complex in unit tests, so we just verify the basic structure.
    }

    #[test]
    fn test_hot_reload_config_defaults() {
        let config = HotReloadConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.debounce_ms, 1000);
    }
}

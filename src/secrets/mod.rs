use crate::error::{GatewayError, Result};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use tracing::{debug, info};

/// Secret string wrapper that prevents accidental exposure
pub type SecretString = Secret<String>;

/// Secrets provider interface
pub trait SecretsProvider {
    /// Get a secret by key
    fn get_secret(&self, key: &str) -> Result<SecretString>;

    /// Check if a secret exists
    fn has_secret(&self, key: &str) -> bool;
}

/// Environment variable secrets provider
#[derive(Debug, Clone)]
pub struct EnvSecretsProvider {
    prefix: String,
}

impl EnvSecretsProvider {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    pub fn default() -> Self {
        Self::new("GATEWAY_SECRET_")
    }
}

impl SecretsProvider for EnvSecretsProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString> {
        let env_key = format!("{}{}", self.prefix, key.to_uppercase());
        env::var(&env_key)
            .map(Secret::new)
            .map_err(|_| GatewayError::Config(format!("Secret '{}' not found in environment", key)))
    }

    fn has_secret(&self, key: &str) -> bool {
        let env_key = format!("{}{}", self.prefix, key.to_uppercase());
        env::var(&env_key).is_ok()
    }
}

/// File-based secrets provider (e.g., for Kubernetes secrets)
#[derive(Debug, Clone)]
pub struct FileSecretsProvider {
    base_path: String,
}

impl FileSecretsProvider {
    pub fn new(base_path: &str) -> Self {
        Self {
            base_path: base_path.to_string(),
        }
    }

    /// Default path for Kubernetes mounted secrets
    pub fn kubernetes_default() -> Self {
        Self::new("/var/run/secrets/gateway")
    }
}

impl SecretsProvider for FileSecretsProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString> {
        let path = Path::new(&self.base_path).join(key);
        fs::read_to_string(&path)
            .map(|s| Secret::new(s.trim().to_string()))
            .map_err(|e| {
                GatewayError::Config(format!(
                    "Failed to read secret from {}: {}",
                    path.display(),
                    e
                ))
            })
    }

    fn has_secret(&self, key: &str) -> bool {
        let path = Path::new(&self.base_path).join(key);
        path.exists()
    }
}

/// In-memory secrets provider (for testing or simple deployments)
#[derive(Debug, Clone)]
pub struct InMemorySecretsProvider {
    secrets: HashMap<String, SecretString>,
}

impl InMemorySecretsProvider {
    pub fn new(secrets: HashMap<String, String>) -> Self {
        Self {
            secrets: secrets
                .into_iter()
                .map(|(k, v)| (k, Secret::new(v)))
                .collect(),
        }
    }

    pub fn empty() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }
}

impl SecretsProvider for InMemorySecretsProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString> {
        self.secrets
            .get(key)
            .cloned()
            .ok_or_else(|| GatewayError::Config(format!("Secret '{}' not found", key)))
    }

    fn has_secret(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }
}

/// Multi-provider secrets manager with fallback chain
pub struct SecretsManager {
    providers: Vec<Box<dyn SecretsProvider + Send + Sync>>,
}

impl SecretsManager {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a provider to the chain (checked in order)
    pub fn add_provider<P: SecretsProvider + Send + Sync + 'static>(mut self, provider: P) -> Self {
        self.providers.push(Box::new(provider));
        self
    }

    /// Get a secret, trying each provider in order
    pub fn get_secret(&self, key: &str) -> Result<SecretString> {
        for provider in &self.providers {
            if provider.has_secret(key) {
                return provider.get_secret(key);
            }
        }
        Err(GatewayError::Config(format!(
            "Secret '{}' not found in any provider",
            key
        )))
    }

    /// Check if a secret exists in any provider
    pub fn has_secret(&self, key: &str) -> bool {
        self.providers.iter().any(|p| p.has_secret(key))
    }

    /// Build a default secrets manager with common providers
    pub fn default() -> Self {
        info!("Initializing default secrets manager");

        let mut manager = Self::new();

        // 1. Try environment variables first
        debug!("Adding environment secrets provider");
        manager = manager.add_provider(EnvSecretsProvider::default());

        // 2. Try Kubernetes secrets if the directory exists
        let k8s_path = "/var/run/secrets/gateway";
        if Path::new(k8s_path).exists() {
            info!("Kubernetes secrets directory found, adding file provider");
            manager = manager.add_provider(FileSecretsProvider::kubernetes_default());
        }

        manager
    }
}

impl Default for SecretsManager {
    fn default() -> Self {
        Self::default()
    }
}

/// Helper function to resolve a secret reference
/// Supports formats:
/// - "secret://key" - Load from secrets manager
/// - "env://VAR" - Load from environment variable directly
/// - "file:///path/to/file" - Load from file
/// - Any other value is returned as-is
pub fn resolve_secret_ref(value: &str, manager: &SecretsManager) -> Result<String> {
    if let Some(key) = value.strip_prefix("secret://") {
        debug!("Resolving secret reference: {}", key);
        manager
            .get_secret(key)
            .map(|s| s.expose_secret().clone())
    } else if let Some(env_var) = value.strip_prefix("env://") {
        debug!("Resolving environment variable: {}", env_var);
        env::var(env_var)
            .map_err(|_| GatewayError::Config(format!("Environment variable '{}' not found", env_var)))
    } else if let Some(path) = value.strip_prefix("file://") {
        debug!("Resolving file reference: {}", path);
        fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .map_err(|e| GatewayError::Config(format!("Failed to read file {}: {}", path, e)))
    } else {
        // Return value as-is (plain text)
        Ok(value.to_string())
    }
}

/// Custom deserializer for secret strings
pub fn deserialize_secret<'de, D>(deserializer: D) -> std::result::Result<SecretString, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Secret::new(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_secrets_provider() {
        std::env::set_var("GATEWAY_SECRET_TEST_KEY", "test_value");

        let provider = EnvSecretsProvider::default();
        assert!(provider.has_secret("test_key"));

        let secret = provider.get_secret("test_key").unwrap();
        assert_eq!(secret.expose_secret(), "test_value");

        std::env::remove_var("GATEWAY_SECRET_TEST_KEY");
    }

    #[test]
    fn test_in_memory_provider() {
        let mut secrets = HashMap::new();
        secrets.insert("key1".to_string(), "value1".to_string());
        secrets.insert("key2".to_string(), "value2".to_string());

        let provider = InMemorySecretsProvider::new(secrets);

        assert!(provider.has_secret("key1"));
        assert!(provider.has_secret("key2"));
        assert!(!provider.has_secret("key3"));

        let secret = provider.get_secret("key1").unwrap();
        assert_eq!(secret.expose_secret(), "value1");
    }

    #[test]
    fn test_secrets_manager_fallback() {
        let mut secrets = HashMap::new();
        secrets.insert("from_memory".to_string(), "memory_value".to_string());

        std::env::set_var("GATEWAY_SECRET_FROM_ENV", "env_value");

        let manager = SecretsManager::new()
            .add_provider(InMemorySecretsProvider::new(secrets))
            .add_provider(EnvSecretsProvider::default());

        // Should find in first provider
        let secret1 = manager.get_secret("from_memory").unwrap();
        assert_eq!(secret1.expose_secret(), "memory_value");

        // Should fallback to second provider
        let secret2 = manager.get_secret("from_env").unwrap();
        assert_eq!(secret2.expose_secret(), "env_value");

        // Should fail (not in any provider)
        assert!(manager.get_secret("not_found").is_err());

        std::env::remove_var("GATEWAY_SECRET_FROM_ENV");
    }

    #[test]
    fn test_resolve_secret_ref() {
        std::env::set_var("TEST_VAR", "env_value");

        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "secret_value".to_string());

        let manager = SecretsManager::new()
            .add_provider(InMemorySecretsProvider::new(secrets));

        // Secret reference
        let result = resolve_secret_ref("secret://api_key", &manager).unwrap();
        assert_eq!(result, "secret_value");

        // Environment reference
        let result = resolve_secret_ref("env://TEST_VAR", &manager).unwrap();
        assert_eq!(result, "env_value");

        // Plain text
        let result = resolve_secret_ref("plain_text", &manager).unwrap();
        assert_eq!(result, "plain_text");

        std::env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_file_secrets_provider() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "file_secret_value").unwrap();

        let temp_dir = file.path().parent().unwrap();
        let file_name = file.path().file_name().unwrap().to_str().unwrap();

        let provider = FileSecretsProvider::new(temp_dir.to_str().unwrap());

        assert!(provider.has_secret(file_name));
        let secret = provider.get_secret(file_name).unwrap();
        assert_eq!(secret.expose_secret(), "file_secret_value");
    }
}

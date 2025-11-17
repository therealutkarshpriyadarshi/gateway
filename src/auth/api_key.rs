use crate::config::ApiKeyConfig;
use crate::error::{GatewayError, Result};
use axum::http::HeaderMap;
use redis::{aio::ConnectionManager, AsyncCommands};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{AuthMethodType, AuthResult};

/// API key validator with in-memory and Redis support
#[derive(Clone)]
pub struct ApiKeyValidator {
    header_name: String,
    in_memory_keys: Arc<RwLock<HashMap<String, ApiKeyInfo>>>,
    redis_client: Option<Arc<RedisKeyStore>>,
}

#[derive(Debug, Clone)]
struct ApiKeyInfo {
    description: String,
    metadata: HashMap<String, serde_json::Value>,
}

/// Redis-backed key store
pub struct RedisKeyStore {
    connection: Arc<RwLock<ConnectionManager>>,
    prefix: String,
}

impl ApiKeyValidator {
    /// Create a new API key validator from configuration
    pub async fn new(config: &ApiKeyConfig) -> Result<Self> {
        // Load in-memory keys
        let mut in_memory_keys = HashMap::new();
        for (key, description) in &config.keys {
            in_memory_keys.insert(
                key.clone(),
                ApiKeyInfo {
                    description: description.clone(),
                    metadata: HashMap::new(),
                },
            );
        }

        // Initialize Redis client if configured
        let redis_client = if let Some(redis_config) = &config.redis {
            let client = redis::Client::open(redis_config.url.as_str()).map_err(|e| {
                GatewayError::Config(format!("Failed to create Redis client: {}", e))
            })?;

            let connection = ConnectionManager::new(client).await.map_err(|e| {
                GatewayError::Config(format!("Failed to connect to Redis: {}", e))
            })?;

            Some(Arc::new(RedisKeyStore {
                connection: Arc::new(RwLock::new(connection)),
                prefix: redis_config.prefix.clone(),
            }))
        } else {
            None
        };

        Ok(Self {
            header_name: config.header.clone(),
            in_memory_keys: Arc::new(RwLock::new(in_memory_keys)),
            redis_client,
        })
    }

    /// Validate an API key from request headers
    pub async fn validate(&self, headers: &HeaderMap) -> Result<AuthResult> {
        let api_key = self.extract_api_key(headers)?;

        // Check in-memory keys first
        let in_memory = self.in_memory_keys.read().await;
        if let Some(key_info) = in_memory.get(&api_key) {
            return Ok(AuthResult {
                user_id: api_key.clone(),
                method: AuthMethodType::ApiKey,
                metadata: key_info.metadata.clone(),
            });
        }
        drop(in_memory);

        // Check Redis if configured
        if let Some(redis_store) = &self.redis_client {
            if let Some(key_info) = redis_store.get_key(&api_key).await? {
                return Ok(AuthResult {
                    user_id: api_key,
                    method: AuthMethodType::ApiKey,
                    metadata: key_info,
                });
            }
        }

        Err(GatewayError::InvalidApiKey)
    }

    /// Extract API key from request headers
    fn extract_api_key(&self, headers: &HeaderMap) -> Result<String> {
        let header_value = headers
            .get(&self.header_name)
            .ok_or(GatewayError::MissingCredentials)?;

        let api_key = header_value
            .to_str()
            .map_err(|_| GatewayError::InvalidApiKey)?
            .to_string();

        if api_key.is_empty() {
            return Err(GatewayError::InvalidApiKey);
        }

        Ok(api_key)
    }

    /// Add a new API key (in-memory)
    pub async fn add_key(&self, key: String, description: String) {
        let mut keys = self.in_memory_keys.write().await;
        keys.insert(
            key,
            ApiKeyInfo {
                description,
                metadata: HashMap::new(),
            },
        );
    }

    /// Remove an API key (in-memory)
    pub async fn remove_key(&self, key: &str) -> bool {
        let mut keys = self.in_memory_keys.write().await;
        keys.remove(key).is_some()
    }

    /// Check if a key exists (in-memory or Redis)
    pub async fn key_exists(&self, key: &str) -> Result<bool> {
        // Check in-memory first
        let in_memory = self.in_memory_keys.read().await;
        if in_memory.contains_key(key) {
            return Ok(true);
        }
        drop(in_memory);

        // Check Redis if configured
        if let Some(redis_store) = &self.redis_client {
            return redis_store.key_exists(key).await;
        }

        Ok(false)
    }
}

impl RedisKeyStore {
    /// Get API key information from Redis
    async fn get_key(&self, key: &str) -> Result<Option<HashMap<String, serde_json::Value>>> {
        let mut conn = self.connection.write().await;
        let redis_key = format!("{}{}", self.prefix, key);

        let exists: bool = conn
            .exists(&redis_key)
            .await
            .map_err(|e| GatewayError::Internal(format!("Redis error: {}", e)))?;

        if !exists {
            return Ok(None);
        }

        // Get key metadata (stored as JSON)
        let metadata_json: Option<String> = conn
            .get(&redis_key)
            .await
            .map_err(|e| GatewayError::Internal(format!("Redis error: {}", e)))?;

        if let Some(json) = metadata_json {
            let metadata: HashMap<String, serde_json::Value> =
                serde_json::from_str(&json).unwrap_or_default();
            Ok(Some(metadata))
        } else {
            Ok(Some(HashMap::new()))
        }
    }

    /// Check if a key exists in Redis
    async fn key_exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.connection.write().await;
        let redis_key = format!("{}{}", self.prefix, key);

        conn.exists(&redis_key)
            .await
            .map_err(|e| GatewayError::Internal(format!("Redis error: {}", e)))
    }

    /// Store an API key in Redis
    #[allow(dead_code)]
    async fn set_key(
        &self,
        key: &str,
        metadata: &HashMap<String, serde_json::Value>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut conn = self.connection.write().await;
        let redis_key = format!("{}{}", self.prefix, key);
        let metadata_json = serde_json::to_string(metadata)
            .map_err(|e| GatewayError::Serialization(format!("Failed to serialize metadata: {}", e)))?;

        if let Some(ttl) = ttl_seconds {
            let _: () = conn.set_ex(&redis_key, metadata_json, ttl)
                .await
                .map_err(|e| GatewayError::Internal(format!("Redis error: {}", e)))?;
        } else {
            let _: () = conn.set(&redis_key, metadata_json)
                .await
                .map_err(|e| GatewayError::Internal(format!("Redis error: {}", e)))?;
        }

        Ok(())
    }

    /// Delete an API key from Redis
    #[allow(dead_code)]
    async fn delete_key(&self, key: &str) -> Result<bool> {
        let mut conn = self.connection.write().await;
        let redis_key = format!("{}{}", self.prefix, key);

        let deleted: i32 = conn
            .del(&redis_key)
            .await
            .map_err(|e| GatewayError::Internal(format!("Redis error: {}", e)))?;

        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ApiKeyConfig;

    #[tokio::test]
    async fn test_in_memory_api_key_validation() {
        let mut keys = HashMap::new();
        keys.insert("test-key-123".to_string(), "Test API key".to_string());

        let config = ApiKeyConfig {
            header: "X-API-Key".to_string(),
            keys,
            redis: None,
        };

        let validator = ApiKeyValidator::new(&config).await.unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("X-API-Key", "test-key-123".parse().unwrap());

        let result = validator.validate(&headers).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().user_id, "test-key-123");
    }

    #[tokio::test]
    async fn test_invalid_api_key() {
        let config = ApiKeyConfig {
            header: "X-API-Key".to_string(),
            keys: HashMap::new(),
            redis: None,
        };

        let validator = ApiKeyValidator::new(&config).await.unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("X-API-Key", "invalid-key".parse().unwrap());

        let result = validator.validate(&headers).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GatewayError::InvalidApiKey));
    }

    #[tokio::test]
    async fn test_missing_api_key_header() {
        let config = ApiKeyConfig {
            header: "X-API-Key".to_string(),
            keys: HashMap::new(),
            redis: None,
        };

        let validator = ApiKeyValidator::new(&config).await.unwrap();
        let headers = HeaderMap::new();

        let result = validator.validate(&headers).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::MissingCredentials
        ));
    }

    #[tokio::test]
    async fn test_add_and_remove_key() {
        let config = ApiKeyConfig {
            header: "X-API-Key".to_string(),
            keys: HashMap::new(),
            redis: None,
        };

        let validator = ApiKeyValidator::new(&config).await.unwrap();

        // Add a key
        validator
            .add_key("new-key".to_string(), "New test key".to_string())
            .await;

        // Verify it exists
        let exists = validator.key_exists("new-key").await.unwrap();
        assert!(exists);

        // Remove the key
        let removed = validator.remove_key("new-key").await;
        assert!(removed);

        // Verify it's gone
        let exists = validator.key_exists("new-key").await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_custom_header_name() {
        let mut keys = HashMap::new();
        keys.insert("test-key".to_string(), "Test".to_string());

        let config = ApiKeyConfig {
            header: "X-Custom-API-Key".to_string(),
            keys,
            redis: None,
        };

        let validator = ApiKeyValidator::new(&config).await.unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("X-Custom-API-Key", "test-key".parse().unwrap());

        let result = validator.validate(&headers).await;
        assert!(result.is_ok());
    }
}

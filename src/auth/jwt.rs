use crate::config::JwtConfig;
use crate::error::{GatewayError, Result};
use axum::http::HeaderMap;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{AuthMethodType, AuthResult};

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Issuer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Audience
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (Unix timestamp)
    pub exp: usize,
    /// Issued at (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<usize>,
    /// Additional custom claims
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// JWT validator
pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
    algorithm: Algorithm,
}

impl JwtValidator {
    /// Create a new JWT validator from configuration
    pub fn new(config: &JwtConfig) -> Result<Self> {
        let algorithm = Self::parse_algorithm(&config.algorithm)?;

        let decoding_key = match algorithm {
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
                let secret = config.secret.as_ref().ok_or_else(|| {
                    GatewayError::Config(
                        "JWT secret is required for HS256/HS384/HS512 algorithms".to_string(),
                    )
                })?;
                DecodingKey::from_secret(secret.as_bytes())
            }
            Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
                let public_key = config.public_key.as_ref().ok_or_else(|| {
                    GatewayError::Config(
                        "JWT public key is required for RS256/RS384/RS512 algorithms".to_string(),
                    )
                })?;
                DecodingKey::from_rsa_pem(public_key.as_bytes()).map_err(|e| {
                    GatewayError::Config(format!("Invalid RSA public key: {}", e))
                })?
            }
            _ => {
                return Err(GatewayError::Config(format!(
                    "Unsupported JWT algorithm: {}",
                    config.algorithm
                )))
            }
        };

        let mut validation = Validation::new(algorithm);

        // Configure issuer validation
        if let Some(issuer) = &config.issuer {
            validation.set_issuer(&[issuer]);
        }

        // Configure audience validation
        if let Some(audience) = &config.audience {
            validation.set_audience(&[audience]);
        }

        // If issuer or audience are not specified, we don't validate them
        validation.validate_exp = true; // Always validate expiration

        Ok(Self {
            decoding_key,
            validation,
            algorithm,
        })
    }

    /// Validate a JWT token from request headers
    pub async fn validate(&self, headers: &HeaderMap) -> Result<AuthResult> {
        // Extract token from Authorization header
        let token = self.extract_token(headers)?;

        // Decode and validate the token
        let token_data = decode::<Claims>(&token, &self.decoding_key, &self.validation)
            .map_err(|e| GatewayError::InvalidToken(format!("Token validation failed: {}", e)))?;

        let claims = token_data.claims;

        // Convert extra claims to metadata
        let mut metadata = HashMap::new();
        for (key, value) in claims.extra.iter() {
            metadata.insert(key.clone(), value.clone());
        }

        // Add standard claims to metadata
        if let Some(iss) = &claims.iss {
            metadata.insert("iss".to_string(), serde_json::Value::String(iss.clone()));
        }
        if let Some(aud) = &claims.aud {
            metadata.insert("aud".to_string(), serde_json::Value::String(aud.clone()));
        }
        metadata.insert("exp".to_string(), serde_json::Value::Number(claims.exp.into()));

        Ok(AuthResult {
            user_id: claims.sub,
            method: AuthMethodType::Jwt,
            metadata,
        })
    }

    /// Extract JWT token from Authorization header
    fn extract_token(&self, headers: &HeaderMap) -> Result<String> {
        let auth_header = headers
            .get("authorization")
            .or_else(|| headers.get("Authorization"))
            .ok_or(GatewayError::MissingCredentials)?;

        let auth_str = auth_header
            .to_str()
            .map_err(|_| GatewayError::InvalidToken("Invalid authorization header".to_string()))?;

        // Check for Bearer token
        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            Ok(token.to_string())
        } else if let Some(token) = auth_str.strip_prefix("bearer ") {
            Ok(token.to_string())
        } else {
            Err(GatewayError::InvalidToken(
                "Authorization header must start with 'Bearer '".to_string(),
            ))
        }
    }

    /// Parse algorithm string to Algorithm enum
    fn parse_algorithm(algo: &str) -> Result<Algorithm> {
        match algo.to_uppercase().as_str() {
            "HS256" => Ok(Algorithm::HS256),
            "HS384" => Ok(Algorithm::HS384),
            "HS512" => Ok(Algorithm::HS512),
            "RS256" => Ok(Algorithm::RS256),
            "RS384" => Ok(Algorithm::RS384),
            "RS512" => Ok(Algorithm::RS512),
            _ => Err(GatewayError::Config(format!(
                "Unsupported algorithm: {}",
                algo
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(secret: &str, claims: &Claims) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_jwt_validator_creation_hs256() {
        let config = JwtConfig {
            secret: Some("test-secret".to_string()),
            public_key: None,
            algorithm: "HS256".to_string(),
            issuer: None,
            audience: None,
        };

        let validator = JwtValidator::new(&config);
        assert!(validator.is_ok());
    }

    #[test]
    fn test_jwt_validator_missing_secret() {
        let config = JwtConfig {
            secret: None,
            public_key: None,
            algorithm: "HS256".to_string(),
            issuer: None,
            audience: None,
        };

        let validator = JwtValidator::new(&config);
        assert!(validator.is_err());
    }

    #[tokio::test]
    async fn test_validate_valid_token() {
        let secret = "test-secret-key";
        let config = JwtConfig {
            secret: Some(secret.to_string()),
            public_key: None,
            algorithm: "HS256".to_string(),
            issuer: None,
            audience: None,
        };

        let validator = JwtValidator::new(&config).unwrap();

        let claims = Claims {
            sub: "user123".to_string(),
            iss: None,
            aud: None,
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            iat: Some(chrono::Utc::now().timestamp() as usize),
            extra: HashMap::new(),
        };

        let token = create_test_token(secret, &claims);

        let mut headers = HeaderMap::new();
        headers.insert("Authorization", format!("Bearer {}", token).parse().unwrap());

        let result = validator.validate(&headers).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().user_id, "user123");
    }

    #[tokio::test]
    async fn test_validate_expired_token() {
        let secret = "test-secret-key";
        let config = JwtConfig {
            secret: Some(secret.to_string()),
            public_key: None,
            algorithm: "HS256".to_string(),
            issuer: None,
            audience: None,
        };

        let validator = JwtValidator::new(&config).unwrap();

        let claims = Claims {
            sub: "user123".to_string(),
            iss: None,
            aud: None,
            exp: (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp() as usize,
            iat: Some(chrono::Utc::now().timestamp() as usize),
            extra: HashMap::new(),
        };

        let token = create_test_token(secret, &claims);

        let mut headers = HeaderMap::new();
        headers.insert("Authorization", format!("Bearer {}", token).parse().unwrap());

        let result = validator.validate(&headers).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_missing_header() {
        let config = JwtConfig {
            secret: Some("test-secret".to_string()),
            public_key: None,
            algorithm: "HS256".to_string(),
            issuer: None,
            audience: None,
        };

        let validator = JwtValidator::new(&config).unwrap();
        let headers = HeaderMap::new();

        let result = validator.validate(&headers).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GatewayError::MissingCredentials));
    }

    #[tokio::test]
    async fn test_validate_invalid_bearer_format() {
        let config = JwtConfig {
            secret: Some("test-secret".to_string()),
            public_key: None,
            algorithm: "HS256".to_string(),
            issuer: None,
            audience: None,
        };

        let validator = JwtValidator::new(&config).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "InvalidToken".parse().unwrap());

        let result = validator.validate(&headers).await;
        assert!(result.is_err());
    }
}

use crate::error::{GatewayError, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// TLS configuration for the gateway
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TlsConfig {
    /// Path to the TLS certificate file (PEM format)
    pub cert_path: String,

    /// Path to the TLS private key file (PEM format)
    pub key_path: String,

    /// Enable mutual TLS (client certificate verification)
    #[serde(default)]
    pub enable_mtls: bool,

    /// Path to the CA certificate for client verification (required if mTLS enabled)
    pub ca_cert_path: Option<String>,

    /// Require client certificate (if false, client cert is optional)
    #[serde(default = "default_require_client_cert")]
    pub require_client_cert: bool,
}

fn default_require_client_cert() -> bool {
    true
}

/// Load certificates from a PEM file
fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let file = File::open(path).map_err(|e| {
        GatewayError::Config(format!("Failed to open certificate file {}: {}", path.display(), e))
    })?;

    let mut reader = BufReader::new(file);
    let certs = certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            GatewayError::Config(format!(
                "Failed to parse certificates from {}: {}",
                path.display(),
                e
            ))
        })?;

    if certs.is_empty() {
        return Err(GatewayError::Config(format!(
            "No certificates found in {}",
            path.display()
        )));
    }

    info!("Loaded {} certificate(s) from {}", certs.len(), path.display());
    Ok(certs)
}

/// Load private key from a PEM file
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let file = File::open(path).map_err(|e| {
        GatewayError::Config(format!("Failed to open private key file {}: {}", path.display(), e))
    })?;

    let mut reader = BufReader::new(file);

    // Try PKCS8 first
    let keys = pkcs8_private_keys(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            GatewayError::Config(format!(
                "Failed to parse PKCS8 private key from {}: {}",
                path.display(),
                e
            ))
        })?;

    if keys.is_empty() {
        // Try RSA format
        let file = File::open(path).map_err(|e| {
            GatewayError::Config(format!(
                "Failed to reopen private key file {}: {}",
                path.display(),
                e
            ))
        })?;
        let mut reader = BufReader::new(file);
        let rsa_keys = rsa_private_keys(&mut reader)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                GatewayError::Config(format!(
                    "Failed to parse RSA private key from {}: {}",
                    path.display(),
                    e
                ))
            })?;

        // Convert RSA keys to PKCS8 format
        if !rsa_keys.is_empty() {
            return Ok(PrivateKeyDer::Pkcs1(rsa_keys.into_iter().next().unwrap()));
        }
    }

    if keys.is_empty() {
        return Err(GatewayError::Config(format!(
            "No private keys found in {}",
            path.display()
        )));
    }

    if keys.len() > 1 {
        warn!(
            "Found {} private keys in {}, using the first one",
            keys.len(),
            path.display()
        );
    }

    info!("Loaded private key from {}", path.display());
    Ok(PrivateKeyDer::Pkcs8(keys.into_iter().next().unwrap()))
}

/// Load CA certificates for client verification
fn load_ca_certs(path: &Path) -> Result<RootCertStore> {
    let file = File::open(path).map_err(|e| {
        GatewayError::Config(format!("Failed to open CA certificate file {}: {}", path.display(), e))
    })?;

    let mut reader = BufReader::new(file);
    let certs = certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            GatewayError::Config(format!(
                "Failed to parse CA certificates from {}: {}",
                path.display(),
                e
            ))
        })?;

    let mut root_store = RootCertStore::empty();
    for cert in certs {
        root_store.add(cert).map_err(|e| {
            GatewayError::Config(format!(
                "Failed to add CA certificate to root store: {}",
                e
            ))
        })?;
    }

    info!("Loaded CA certificates from {}", path.display());
    Ok(root_store)
}

/// Build TLS server configuration
pub fn build_tls_config(tls_config: &TlsConfig) -> Result<ServerConfig> {
    info!("Building TLS configuration");

    // Load server certificate and key
    let certs = load_certs(Path::new(&tls_config.cert_path))?;
    let key = load_private_key(Path::new(&tls_config.key_path))?;

    // Build server config
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| GatewayError::Config(format!("Failed to build TLS config: {}", e)))?;

    // Configure mTLS if enabled
    if tls_config.enable_mtls {
        info!("Configuring mutual TLS (client certificate verification)");

        let ca_cert_path = tls_config.ca_cert_path.as_ref().ok_or_else(|| {
            GatewayError::Config(
                "ca_cert_path is required when enable_mtls is true".to_string(),
            )
        })?;

        let root_store = load_ca_certs(Path::new(ca_cert_path))?;

        let client_verifier = if tls_config.require_client_cert {
            info!("Client certificates are required");
            WebPkiClientVerifier::builder(Arc::new(root_store))
                .build()
                .map_err(|e| {
                    GatewayError::Config(format!("Failed to build client verifier: {}", e))
                })?
        } else {
            info!("Client certificates are optional");
            WebPkiClientVerifier::builder(Arc::new(root_store))
                .allow_unauthenticated()
                .build()
                .map_err(|e| {
                    GatewayError::Config(format!("Failed to build client verifier: {}", e))
                })?
        };

        // Rebuild config with client auth
        let certs = load_certs(Path::new(&tls_config.cert_path))?;
        let key = load_private_key(Path::new(&tls_config.key_path))?;

        config = ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certs, key)
            .map_err(|e| {
                GatewayError::Config(format!("Failed to build TLS config with mTLS: {}", e))
            })?;
    }

    // Enable ALPN for HTTP/2 and HTTP/1.1
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    info!("TLS configuration built successfully");
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tls_config_deserialization() {
        let yaml = r#"
cert_path: "/path/to/cert.pem"
key_path: "/path/to/key.pem"
enable_mtls: true
ca_cert_path: "/path/to/ca.pem"
require_client_cert: false
"#;

        let config: TlsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.cert_path, "/path/to/cert.pem");
        assert_eq!(config.key_path, "/path/to/key.pem");
        assert!(config.enable_mtls);
        assert_eq!(config.ca_cert_path, Some("/path/to/ca.pem".to_string()));
        assert!(!config.require_client_cert);
    }

    #[test]
    fn test_default_require_client_cert() {
        let yaml = r#"
cert_path: "/path/to/cert.pem"
key_path: "/path/to/key.pem"
"#;

        let config: TlsConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.require_client_cert); // Default is true
        assert!(!config.enable_mtls); // Default is false
    }
}

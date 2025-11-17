use crate::error::{GatewayError, Result};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::str::FromStr;
use tracing::{debug, warn};

/// IP filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IpFilterConfig {
    /// List of allowed IP addresses or CIDR ranges
    #[serde(default)]
    pub whitelist: Vec<String>,
    /// List of blocked IP addresses or CIDR ranges
    #[serde(default)]
    pub blacklist: Vec<String>,
    /// Default action when IP doesn't match any rule (allow or deny)
    #[serde(default = "default_action")]
    pub default_action: IpFilterAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum IpFilterAction {
    #[default]
    Allow,
    Deny,
}

fn default_action() -> IpFilterAction {
    IpFilterAction::Allow
}

/// IP filter service for checking client IPs against whitelist/blacklist
#[derive(Debug)]
pub struct IpFilterService {
    config: IpFilterConfig,
    whitelist_ranges: Vec<IpRange>,
    blacklist_ranges: Vec<IpRange>,
}

/// Represents an IP address or CIDR range
#[derive(Debug, Clone)]
enum IpRange {
    Single(IpAddr),
    Cidr { network: IpAddr, prefix_len: u8 },
}

impl IpRange {
    /// Parse an IP or CIDR string into an IpRange
    fn parse(s: &str) -> Result<Self> {
        if let Some((network_str, prefix_str)) = s.split_once('/') {
            // CIDR notation
            let network = IpAddr::from_str(network_str).map_err(|e| {
                GatewayError::Config(format!("Invalid IP address in CIDR '{}': {}", s, e))
            })?;

            let prefix_len = prefix_str.parse::<u8>().map_err(|e| {
                GatewayError::Config(format!("Invalid prefix length in CIDR '{}': {}", s, e))
            })?;

            // Validate prefix length
            match network {
                IpAddr::V4(_) if prefix_len > 32 => {
                    return Err(GatewayError::Config(format!(
                        "Invalid IPv4 prefix length {}: must be 0-32",
                        prefix_len
                    )));
                }
                IpAddr::V6(_) if prefix_len > 128 => {
                    return Err(GatewayError::Config(format!(
                        "Invalid IPv6 prefix length {}: must be 0-128",
                        prefix_len
                    )));
                }
                _ => {}
            }

            Ok(IpRange::Cidr {
                network,
                prefix_len,
            })
        } else {
            // Single IP address
            let ip = IpAddr::from_str(s).map_err(|e| {
                GatewayError::Config(format!("Invalid IP address '{}': {}", s, e))
            })?;
            Ok(IpRange::Single(ip))
        }
    }

    /// Check if an IP address matches this range
    fn contains(&self, ip: &IpAddr) -> bool {
        match self {
            IpRange::Single(range_ip) => ip == range_ip,
            IpRange::Cidr {
                network,
                prefix_len,
            } => {
                // For simplicity, only match same IP version
                match (network, ip) {
                    (IpAddr::V4(net), IpAddr::V4(addr)) => {
                        let net_bits = u32::from_be_bytes(net.octets());
                        let addr_bits = u32::from_be_bytes(addr.octets());
                        let mask = if *prefix_len == 0 {
                            0
                        } else {
                            !0u32 << (32 - prefix_len)
                        };
                        (net_bits & mask) == (addr_bits & mask)
                    }
                    (IpAddr::V6(net), IpAddr::V6(addr)) => {
                        let net_bits = u128::from_be_bytes(net.octets());
                        let addr_bits = u128::from_be_bytes(addr.octets());
                        let mask = if *prefix_len == 0 {
                            0
                        } else {
                            !0u128 << (128 - prefix_len)
                        };
                        (net_bits & mask) == (addr_bits & mask)
                    }
                    _ => false, // Different IP versions
                }
            }
        }
    }
}

impl IpFilterService {
    /// Create a new IP filter service from configuration
    pub fn new(config: IpFilterConfig) -> Result<Self> {
        // Parse whitelist ranges
        let whitelist_ranges: Result<Vec<IpRange>> =
            config.whitelist.iter().map(|s| IpRange::parse(s)).collect();

        // Parse blacklist ranges
        let blacklist_ranges: Result<Vec<IpRange>> =
            config.blacklist.iter().map(|s| IpRange::parse(s)).collect();

        Ok(Self {
            config,
            whitelist_ranges: whitelist_ranges?,
            blacklist_ranges: blacklist_ranges?,
        })
    }

    /// Check if an IP address is allowed
    pub fn is_allowed(&self, ip: &IpAddr) -> bool {
        // Check blacklist first (highest priority)
        for range in &self.blacklist_ranges {
            if range.contains(ip) {
                debug!(ip = %ip, "IP blocked by blacklist");
                return false;
            }
        }

        // If whitelist is not empty, IP must be in whitelist
        if !self.whitelist_ranges.is_empty() {
            for range in &self.whitelist_ranges {
                if range.contains(ip) {
                    debug!(ip = %ip, "IP allowed by whitelist");
                    return true;
                }
            }
            // IP not in whitelist
            warn!(ip = %ip, "IP not in whitelist, denying");
            return false;
        }

        // No whitelist configured, use default action
        match self.config.default_action {
            IpFilterAction::Allow => {
                debug!(ip = %ip, "IP allowed by default action");
                true
            }
            IpFilterAction::Deny => {
                warn!(ip = %ip, "IP denied by default action");
                false
            }
        }
    }

    /// Check if filtering is enabled (has any rules)
    pub fn is_enabled(&self) -> bool {
        !self.whitelist_ranges.is_empty() || !self.blacklist_ranges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_ip_whitelist() {
        let config = IpFilterConfig {
            whitelist: vec!["192.168.1.1".to_string()],
            blacklist: vec![],
            default_action: IpFilterAction::Deny,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(service.is_allowed(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(!service.is_allowed(&IpAddr::from_str("192.168.1.2").unwrap()));
    }

    #[test]
    fn test_cidr_whitelist() {
        let config = IpFilterConfig {
            whitelist: vec!["192.168.1.0/24".to_string()],
            blacklist: vec![],
            default_action: IpFilterAction::Deny,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(service.is_allowed(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(service.is_allowed(&IpAddr::from_str("192.168.1.255").unwrap()));
        assert!(!service.is_allowed(&IpAddr::from_str("192.168.2.1").unwrap()));
    }

    #[test]
    fn test_blacklist_override_whitelist() {
        let config = IpFilterConfig {
            whitelist: vec!["192.168.1.0/24".to_string()],
            blacklist: vec!["192.168.1.100".to_string()],
            default_action: IpFilterAction::Deny,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(service.is_allowed(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(!service.is_allowed(&IpAddr::from_str("192.168.1.100").unwrap()));
    }

    #[test]
    fn test_blacklist_only() {
        let config = IpFilterConfig {
            whitelist: vec![],
            blacklist: vec!["10.0.0.1".to_string(), "10.0.0.0/24".to_string()],
            default_action: IpFilterAction::Allow,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(!service.is_allowed(&IpAddr::from_str("10.0.0.1").unwrap()));
        assert!(!service.is_allowed(&IpAddr::from_str("10.0.0.50").unwrap()));
        assert!(service.is_allowed(&IpAddr::from_str("10.0.1.1").unwrap()));
    }

    #[test]
    fn test_default_allow() {
        let config = IpFilterConfig {
            whitelist: vec![],
            blacklist: vec![],
            default_action: IpFilterAction::Allow,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(service.is_allowed(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(service.is_allowed(&IpAddr::from_str("10.0.0.1").unwrap()));
    }

    #[test]
    fn test_default_deny() {
        let config = IpFilterConfig {
            whitelist: vec![],
            blacklist: vec![],
            default_action: IpFilterAction::Deny,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(!service.is_allowed(&IpAddr::from_str("192.168.1.1").unwrap()));
        assert!(!service.is_allowed(&IpAddr::from_str("10.0.0.1").unwrap()));
    }

    #[test]
    fn test_ipv6_support() {
        let config = IpFilterConfig {
            whitelist: vec!["2001:db8::/32".to_string()],
            blacklist: vec![],
            default_action: IpFilterAction::Deny,
        };

        let service = IpFilterService::new(config).unwrap();

        assert!(service.is_allowed(&IpAddr::from_str("2001:db8::1").unwrap()));
        assert!(service.is_allowed(&IpAddr::from_str("2001:db8:ffff::1").unwrap()));
        assert!(!service.is_allowed(&IpAddr::from_str("2001:db9::1").unwrap()));
    }

    #[test]
    fn test_invalid_ip_config() {
        let config = IpFilterConfig {
            whitelist: vec!["invalid-ip".to_string()],
            blacklist: vec![],
            default_action: IpFilterAction::Allow,
        };

        assert!(IpFilterService::new(config).is_err());
    }

    #[test]
    fn test_invalid_cidr_prefix() {
        let config = IpFilterConfig {
            whitelist: vec!["192.168.1.0/33".to_string()], // Invalid prefix for IPv4
            blacklist: vec![],
            default_action: IpFilterAction::Allow,
        };

        assert!(IpFilterService::new(config).is_err());
    }

    #[test]
    fn test_is_enabled() {
        let config1 = IpFilterConfig {
            whitelist: vec!["192.168.1.0/24".to_string()],
            blacklist: vec![],
            default_action: IpFilterAction::Allow,
        };
        assert!(IpFilterService::new(config1).unwrap().is_enabled());

        let config2 = IpFilterConfig {
            whitelist: vec![],
            blacklist: vec!["10.0.0.1".to_string()],
            default_action: IpFilterAction::Allow,
        };
        assert!(IpFilterService::new(config2).unwrap().is_enabled());

        let config3 = IpFilterConfig {
            whitelist: vec![],
            blacklist: vec![],
            default_action: IpFilterAction::Allow,
        };
        assert!(!IpFilterService::new(config3).unwrap().is_enabled());
    }
}

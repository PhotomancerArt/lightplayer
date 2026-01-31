//! Host specifier parsing
//!
//! Parses host specifiers to determine transport type and parameters.
//! Supports websocket (`ws://`, `wss://`) and serial (`serial:`) formats.

use anyhow::{Result, bail};
use std::fmt;

/// Host specifier indicating transport type and connection details
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSpecifier {
    /// WebSocket connection
    WebSocket { url: String },
    /// Serial connection
    Serial { port: Option<String> }, // None = auto-detect
    /// Local in-memory server
    Local,
}

impl HostSpecifier {
    /// Parse a host specifier string
    ///
    /// # Arguments
    ///
    /// * `s` - Host specifier string (e.g., `ws://localhost:2812/`, `serial:auto`)
    ///
    /// # Returns
    ///
    /// * `Ok(HostSpecifier)` if the specifier is valid
    /// * `Err` with a clear error message if invalid
    ///
    /// # Examples
    ///
    /// ```
    /// use lp_client::HostSpecifier;
    ///
    /// let ws = HostSpecifier::parse("ws://localhost:2812/").unwrap();
    /// assert!(ws.is_websocket());
    ///
    /// let serial = HostSpecifier::parse("serial:auto").unwrap();
    /// assert!(serial.is_serial());
    /// ```
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Check for local specifier
        if s.is_empty() || s == "local" {
            return Ok(HostSpecifier::Local);
        }

        // Check for websocket URLs
        if s.starts_with("ws://") || s.starts_with("wss://") {
            return Ok(HostSpecifier::WebSocket { url: s.to_string() });
        }

        // Check for serial specifier
        if s.starts_with("serial:") {
            let port = s.strip_prefix("serial:").unwrap().trim();
            if port.is_empty() || port == "auto" {
                return Ok(HostSpecifier::Serial { port: None });
            }
            return Ok(HostSpecifier::Serial {
                port: Some(port.to_string()),
            });
        }

        bail!(
            "Invalid host specifier: '{s}'. Supported formats: ws://host:port/, wss://host:port/, serial:auto, serial:/dev/ttyUSB1, local"
        )
    }

    /// Check if this is a websocket specifier
    #[allow(dead_code, reason = "Useful helper method for future use")]
    pub fn is_websocket(&self) -> bool {
        matches!(self, HostSpecifier::WebSocket { .. })
    }

    /// Check if this is a serial specifier
    #[allow(dead_code, reason = "Useful helper method for future use")]
    pub fn is_serial(&self) -> bool {
        matches!(self, HostSpecifier::Serial { .. })
    }

    /// Check if this is a local specifier
    #[allow(dead_code, reason = "Useful helper method for future use")]
    pub fn is_local(&self) -> bool {
        matches!(self, HostSpecifier::Local)
    }
}

impl fmt::Display for HostSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostSpecifier::WebSocket { url } => write!(f, "{url}"),
            HostSpecifier::Serial { port: None } => write!(f, "serial:auto"),
            HostSpecifier::Serial { port: Some(port) } => write!(f, "serial:{port}"),
            HostSpecifier::Local => write!(f, "local"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_websocket() {
        let spec = HostSpecifier::parse("ws://localhost:2812/").unwrap();
        assert!(spec.is_websocket());
        assert!(!spec.is_serial());
        match spec {
            HostSpecifier::WebSocket { url } => {
                assert_eq!(url, "ws://localhost:2812/");
            }
            _ => panic!("Expected WebSocket"),
        }
    }

    #[test]
    fn test_parse_websocket_secure() {
        let spec = HostSpecifier::parse("wss://example.com/").unwrap();
        assert!(spec.is_websocket());
        match spec {
            HostSpecifier::WebSocket { url } => {
                assert_eq!(url, "wss://example.com/");
            }
            _ => panic!("Expected WebSocket"),
        }
    }

    #[test]
    fn test_parse_serial_auto() {
        let spec = HostSpecifier::parse("serial:auto").unwrap();
        assert!(spec.is_serial());
        assert!(!spec.is_websocket());
        match spec {
            HostSpecifier::Serial { port: None } => {}
            _ => panic!("Expected Serial with None port"),
        }
    }

    #[test]
    fn test_parse_serial_empty() {
        let spec = HostSpecifier::parse("serial:").unwrap();
        assert!(spec.is_serial());
        match spec {
            HostSpecifier::Serial { port: None } => {}
            _ => panic!("Expected Serial with None port"),
        }
    }

    #[test]
    fn test_parse_serial_with_port() {
        let spec = HostSpecifier::parse("serial:/dev/ttyUSB1").unwrap();
        assert!(spec.is_serial());
        match spec {
            HostSpecifier::Serial { port: Some(port) } => {
                assert_eq!(port, "/dev/ttyUSB1");
            }
            _ => panic!("Expected Serial with port"),
        }
    }

    #[test]
    fn test_parse_serial_with_whitespace() {
        let spec = HostSpecifier::parse("serial: /dev/ttyUSB1 ").unwrap();
        assert!(spec.is_serial());
        match spec {
            HostSpecifier::Serial { port: Some(port) } => {
                assert_eq!(port, "/dev/ttyUSB1");
            }
            _ => panic!("Expected Serial with port"),
        }
    }

    #[test]
    fn test_parse_invalid() {
        let result = HostSpecifier::parse("invalid");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid host specifier"));
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn test_display_websocket() {
        let spec = HostSpecifier::WebSocket {
            url: "ws://localhost:2812/".to_string(),
        };
        assert_eq!(spec.to_string(), "ws://localhost:2812/");
    }

    #[test]
    fn test_display_serial_auto() {
        let spec = HostSpecifier::Serial { port: None };
        assert_eq!(spec.to_string(), "serial:auto");
    }

    #[test]
    fn test_display_serial_with_port() {
        let spec = HostSpecifier::Serial {
            port: Some("/dev/ttyUSB1".to_string()),
        };
        assert_eq!(spec.to_string(), "serial:/dev/ttyUSB1");
    }

    #[test]
    fn test_parse_websocket_with_trailing_slash() {
        let spec = HostSpecifier::parse("ws://localhost:2812/").unwrap();
        assert!(spec.is_websocket());
    }

    #[test]
    fn test_parse_websocket_without_trailing_slash() {
        let spec = HostSpecifier::parse("ws://localhost:2812").unwrap();
        assert!(spec.is_websocket());
    }

    #[test]
    fn test_parse_local() {
        let spec = HostSpecifier::parse("local").unwrap();
        assert!(spec.is_local());
        assert!(!spec.is_websocket());
        assert!(!spec.is_serial());
    }

    #[test]
    fn test_parse_empty_string() {
        let spec = HostSpecifier::parse("").unwrap();
        assert!(spec.is_local());
    }

    #[test]
    fn test_display_local() {
        let spec = HostSpecifier::Local;
        assert_eq!(spec.to_string(), "local");
    }
}

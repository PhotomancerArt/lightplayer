//! Host specifier parsing
//!
//! Parses host specifiers to determine transport type and parameters.
//! Supports websocket (`ws://`, `wss://`) and serial (`serial:`) formats.

use anyhow::{Result, bail};
use lp_model::DEFAULT_SERIAL_BAUD_RATE;
use std::fmt;

/// Host specifier indicating transport type and connection details
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSpecifier {
    /// WebSocket connection
    WebSocket { url: String },
    /// Serial connection
    Serial {
        port: Option<String>,   // None = auto-detect
        baud_rate: Option<u32>, // None = default to DEFAULT_SERIAL_BAUD_RATE
    },
    /// Local in-memory server
    Local,
    /// Emulator-based serial transport
    Emulator,
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

        // Check for emulator specifier
        if s == "emu" || s == "emulator" {
            return Ok(HostSpecifier::Emulator);
        }

        // Check for websocket URLs
        if s.starts_with("ws://") || s.starts_with("wss://") {
            return Ok(HostSpecifier::WebSocket { url: s.to_string() });
        }

        // Check for serial specifier
        if s.starts_with("serial:") {
            let rest = s.strip_prefix("serial:").unwrap().trim();

            // Split on '?' to separate port from query string
            let (port_str, query_str) = match rest.split_once('?') {
                Some((p, q)) => (p.trim(), Some(q.trim())),
                None => (rest, None),
            };

            let port = if port_str.is_empty() || port_str == "auto" {
                None
            } else {
                Some(port_str.to_string())
            };

            // Parse baud rate from query string
            let baud_rate = if let Some(query) = query_str {
                parse_baud_rate_from_query(query)?
            } else {
                None
            };

            return Ok(HostSpecifier::Serial { port, baud_rate });
        }

        bail!(
            "Invalid host specifier: '{s}'. Supported formats: ws://host:port/, wss://host:port/, serial:auto, serial:/dev/ttyUSB1, serial:/dev/cu.usbmodem2101?baud={DEFAULT_SERIAL_BAUD_RATE}, local, emu"
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

    /// Check if this is an emulator specifier
    #[allow(dead_code, reason = "Useful helper method for future use")]
    pub fn is_emulator(&self) -> bool {
        matches!(self, HostSpecifier::Emulator)
    }

    /// Get baud rate for serial connection, defaulting to DEFAULT_SERIAL_BAUD_RATE
    ///
    /// Returns the configured baud rate, or DEFAULT_SERIAL_BAUD_RATE if not specified.
    pub fn baud_rate(&self) -> u32 {
        match self {
            HostSpecifier::Serial { baud_rate, .. } => {
                baud_rate.unwrap_or(DEFAULT_SERIAL_BAUD_RATE)
            }
            _ => DEFAULT_SERIAL_BAUD_RATE, // Default for non-serial (shouldn't be called)
        }
    }
}

/// Parse baud rate from query string
///
/// Supports format: `baud=115200`
/// Returns None if baud parameter not found or invalid.
fn parse_baud_rate_from_query(query: &str) -> Result<Option<u32>> {
    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            if key.trim() == "baud" {
                let baud = value
                    .trim()
                    .parse::<u32>()
                    .map_err(|e| anyhow::anyhow!("Invalid baud rate '{value}': {e}"))?;
                return Ok(Some(baud));
            }
        }
    }
    Ok(None)
}

impl fmt::Display for HostSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostSpecifier::WebSocket { url } => write!(f, "{url}"),
            HostSpecifier::Serial {
                port: None,
                baud_rate: None,
            } => write!(f, "serial:auto"),
            HostSpecifier::Serial {
                port: None,
                baud_rate: Some(baud),
            } => {
                write!(f, "serial:auto?baud={baud}")
            }
            HostSpecifier::Serial {
                port: Some(port),
                baud_rate: None,
            } => {
                write!(f, "serial:{port}")
            }
            HostSpecifier::Serial {
                port: Some(port),
                baud_rate: Some(baud),
            } => {
                write!(f, "serial:{port}?baud={baud}")
            }
            HostSpecifier::Local => write!(f, "local"),
            HostSpecifier::Emulator => write!(f, "emu"),
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
            HostSpecifier::Serial {
                port: None,
                baud_rate: None,
            } => {}
            _ => panic!("Expected Serial with None port and None baud_rate"),
        }
        assert_eq!(spec.baud_rate(), DEFAULT_SERIAL_BAUD_RATE); // Should default to DEFAULT_SERIAL_BAUD_RATE
    }

    #[test]
    fn test_parse_serial_empty() {
        let spec = HostSpecifier::parse("serial:").unwrap();
        assert!(spec.is_serial());
        match spec {
            HostSpecifier::Serial {
                port: None,
                baud_rate: None,
            } => {}
            _ => panic!("Expected Serial with None port and None baud_rate"),
        }
    }

    #[test]
    fn test_parse_serial_with_port() {
        let spec = HostSpecifier::parse("serial:/dev/ttyUSB1").unwrap();
        assert!(spec.is_serial());
        match &spec {
            HostSpecifier::Serial {
                port: Some(port),
                baud_rate: None,
            } => {
                assert_eq!(port, "/dev/ttyUSB1");
            }
            _ => panic!("Expected Serial with port and None baud_rate"),
        }
        assert_eq!(spec.baud_rate(), DEFAULT_SERIAL_BAUD_RATE); // Should default to DEFAULT_SERIAL_BAUD_RATE
    }

    #[test]
    fn test_parse_serial_with_whitespace() {
        let spec = HostSpecifier::parse("serial: /dev/ttyUSB1 ").unwrap();
        assert!(spec.is_serial());
        match spec {
            HostSpecifier::Serial {
                port: Some(port),
                baud_rate: None,
            } => {
                assert_eq!(port, "/dev/ttyUSB1");
            }
            _ => panic!("Expected Serial with port and None baud_rate"),
        }
    }

    #[test]
    fn test_parse_serial_with_baud_rate() {
        let spec = HostSpecifier::parse("serial:/dev/cu.usbmodem2101?baud=115200").unwrap();
        match &spec {
            HostSpecifier::Serial {
                port: Some(p),
                baud_rate: Some(b),
            } => {
                assert_eq!(p, "/dev/cu.usbmodem2101");
                assert_eq!(*b, 115200);
            }
            _ => panic!("Expected Serial with port and baud_rate"),
        }
        assert_eq!(spec.baud_rate(), 115200);
    }

    #[test]
    fn test_parse_serial_auto_with_baud_rate() {
        let spec = HostSpecifier::parse("serial:auto?baud=9600").unwrap();
        match spec {
            HostSpecifier::Serial {
                port: None,
                baud_rate: Some(b),
            } => {
                assert_eq!(b, 9600);
            }
            _ => panic!("Expected Serial with None port and baud_rate"),
        }
        assert_eq!(spec.baud_rate(), 9600);
    }

    #[test]
    fn test_parse_serial_default_baud_rate() {
        let spec = HostSpecifier::parse("serial:/dev/cu.usbmodem2101").unwrap();
        match spec {
            HostSpecifier::Serial {
                port: Some(_),
                baud_rate: None,
            } => {}
            _ => panic!("Expected Serial with port and None baud_rate"),
        }
        assert_eq!(spec.baud_rate(), DEFAULT_SERIAL_BAUD_RATE); // Should default to DEFAULT_SERIAL_BAUD_RATE
    }

    #[test]
    fn test_parse_serial_invalid_baud_rate() {
        let result = HostSpecifier::parse("serial:/dev/cu.usbmodem2101?baud=invalid");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid baud rate"));
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
        let spec = HostSpecifier::Serial {
            port: None,
            baud_rate: None,
        };
        assert_eq!(spec.to_string(), "serial:auto");
    }

    #[test]
    fn test_display_serial_with_port() {
        let spec = HostSpecifier::Serial {
            port: Some("/dev/ttyUSB1".to_string()),
            baud_rate: None,
        };
        assert_eq!(spec.to_string(), "serial:/dev/ttyUSB1");
    }

    #[test]
    fn test_display_serial_with_baud_rate() {
        let spec = HostSpecifier::Serial {
            port: Some("/dev/cu.usbmodem2101".to_string()),
            baud_rate: Some(115200),
        };
        assert_eq!(spec.to_string(), "serial:/dev/cu.usbmodem2101?baud=115200");
    }

    #[test]
    fn test_display_serial_auto_with_baud_rate() {
        let spec = HostSpecifier::Serial {
            port: None,
            baud_rate: Some(9600),
        };
        assert_eq!(spec.to_string(), "serial:auto?baud=9600");
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
        assert!(!spec.is_emulator());
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

    #[test]
    fn test_parse_emu() {
        let spec = HostSpecifier::parse("emu").unwrap();
        assert!(spec.is_emulator());
        assert!(!spec.is_websocket());
        assert!(!spec.is_serial());
        assert!(!spec.is_local());
    }

    #[test]
    fn test_parse_emulator() {
        let spec = HostSpecifier::parse("emulator").unwrap();
        assert!(spec.is_emulator());
        assert!(!spec.is_websocket());
        assert!(!spec.is_serial());
        assert!(!spec.is_local());
    }

    #[test]
    fn test_display_emulator() {
        let spec = HostSpecifier::Emulator;
        assert_eq!(spec.to_string(), "emu");
    }
}

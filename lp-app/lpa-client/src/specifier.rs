//! Host specifier parsing
//!
//! Parses host specifiers to determine transport type and parameters.
//! Supports websocket (`ws://`, `wss://`, lpc-wire to `lp-cli serve`), serial
//! (`serial:`), a board on the LAN (`lan:<host>[:port]`) and a board through
//! the cloud relay (`relay:<board>[@<origin>]`) formats.

use anyhow::{Result, bail};
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;
use lpc_relay::RelayBoardId;
use std::fmt;

/// The relay a `relay:` address names when it names none.
pub const RELAY_DEFAULT_ORIGIN: &str = "https://lightplayer.app";

/// A LAN board's link port when a `lan:` address names none (Wi-Fi plan Q6:
/// 80, the port a device-served panel will share).
pub const LAN_DEFAULT_PORT: u16 = 80;

/// Host specifier indicating transport type and connection details
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSpecifier {
    /// WebSocket connection (lpc-wire to `lp-cli serve`)
    WebSocket { url: String },
    /// A board on the LAN: a secure lp-link inside a WebSocket to
    /// `ws://<host>:<port>/link`. `host` is an IPv4 address or a `.local`
    /// name, resolved by the OS resolver (mDNS on macOS).
    Lan { host: String, port: u16 },
    /// A board through the cloud relay: the same secure lp-link, inside a
    /// WebSocket to `<origin>/relay/board/<board>` (`wss://` for an
    /// `https://` origin). `board` is its relay id (its MAC).
    Relay { board: RelayBoardId, origin: String },
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
    /// use lpa_client::HostSpecifier;
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

        // A board on the LAN
        if let Some(rest) = s.strip_prefix("lan:") {
            let (host, port) = parse_lan_address(rest.trim())?;
            return Ok(HostSpecifier::Lan { host, port });
        }

        // A board through the cloud relay
        if let Some(rest) = s.strip_prefix("relay:") {
            let (board, origin) = parse_relay_address(rest.trim())?;
            return Ok(HostSpecifier::Relay { board, origin });
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
            "Invalid host specifier: '{s}'. Supported formats: ws://host:port/, wss://host:port/, serial:auto, serial:/dev/ttyUSB1, serial:/dev/cu.usbmodem2101?baud={DEFAULT_SERIAL_BAUD_RATE}, lan:192.168.1.40, lan:lp-3f2a.local[:port], relay:<board-id>[@<origin>], local, emu"
        )
    }

    /// Check if this is a LAN board specifier
    pub fn is_lan(&self) -> bool {
        matches!(self, HostSpecifier::Lan { .. })
    }

    /// Whether this is a board through the cloud relay.
    pub fn is_relay(&self) -> bool {
        matches!(self, HostSpecifier::Relay { .. })
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

/// The board a `relay:` address names: `<board-id>[@<origin>]`, the origin
/// `https://lightplayer.app` when it names none.
fn parse_relay_address(rest: &str) -> Result<(RelayBoardId, String)> {
    const FORM: &str = "relay:<board-id>[@<origin>], e.g. relay:10bda3b08e30 or relay:10bda3b08e30@http://127.0.0.1:2812";
    let (id, origin) = match rest.split_once('@') {
        Some((id, origin)) => (id.trim(), origin.trim()),
        None => (rest, RELAY_DEFAULT_ORIGIN),
    };
    let board = id
        .parse::<RelayBoardId>()
        .map_err(|error| anyhow::anyhow!("'relay:{rest}': {error}; {FORM}"))?;
    if !(origin.starts_with("https://") || origin.starts_with("http://")) {
        bail!("'relay:{rest}': the origin must be http:// or https://; {FORM}");
    }
    Ok((board, origin.trim_end_matches('/').to_string()))
}

/// The board a `lan:` address names: `<host>[:port]`, the port 80 when
/// absent. IPv6 literals are not taken (a board's address is IPv4 or its
/// `.local` name).
fn parse_lan_address(rest: &str) -> Result<(String, u16)> {
    const FORM: &str = "lan:<host>[:port], e.g. lan:192.168.1.40 or lan:lp-3f2a.local";
    if rest.is_empty() {
        bail!("lan: needs the board's address: {FORM}");
    }
    if rest.starts_with("//") || rest.contains('/') {
        bail!("'lan:{rest}' is not an address: {FORM} (no scheme, no path)");
    }
    let (host, port) = match rest.split_once(':') {
        None => (rest, LAN_DEFAULT_PORT),
        Some((host, port)) => {
            if port.contains(':') {
                bail!("'lan:{rest}': IPv6 addresses are not supported; {FORM}");
            }
            let port = port
                .parse::<u16>()
                .ok()
                .filter(|&p| p != 0)
                .ok_or_else(|| anyhow::anyhow!("'lan:{rest}': '{port}' is not a port"))?;
            (host, port)
        }
    };
    if host.is_empty() {
        bail!("lan: needs the board's address: {FORM}");
    }
    Ok((host.to_string(), port))
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
            HostSpecifier::Lan { host, port } if *port == LAN_DEFAULT_PORT => {
                write!(f, "lan:{host}")
            }
            HostSpecifier::Lan { host, port } => write!(f, "lan:{host}:{port}"),
            HostSpecifier::Relay { board, origin } if origin == RELAY_DEFAULT_ORIGIN => {
                write!(f, "relay:{board}")
            }
            HostSpecifier::Relay { board, origin } => write!(f, "relay:{board}@{origin}"),
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
    fn a_lan_address_is_a_host_and_a_port_80_by_default() {
        assert_eq!(
            HostSpecifier::parse("lan:192.168.1.40").unwrap(),
            HostSpecifier::Lan {
                host: "192.168.1.40".to_string(),
                port: 80
            }
        );
        let named = HostSpecifier::parse(" lan:lp-3f2a.local:8080 ").unwrap();
        assert!(named.is_lan());
        assert_eq!(
            named,
            HostSpecifier::Lan {
                host: "lp-3f2a.local".to_string(),
                port: 8080
            }
        );
        assert_eq!(named.to_string(), "lan:lp-3f2a.local:8080");
        assert_eq!(
            HostSpecifier::parse("lan:10.0.0.7:80").unwrap().to_string(),
            "lan:10.0.0.7"
        );
    }

    #[test]
    fn a_bad_lan_address_is_refused_in_words() {
        for bad in [
            "lan:",
            "lan::80",
            "lan:host:0",
            "lan:host:http",
            "lan://10.0.0.7",
            "lan:fe80::1",
        ] {
            let error = HostSpecifier::parse(bad).unwrap_err().to_string();
            assert!(error.contains("lan:"), "{bad}: {error}");
        }
    }

    #[test]
    fn a_relay_address_is_a_board_id_and_an_origin() {
        let board = RelayBoardId([0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30]);
        assert_eq!(
            HostSpecifier::parse("relay:10bda3b08e30").unwrap(),
            HostSpecifier::Relay {
                board,
                origin: RELAY_DEFAULT_ORIGIN.to_string()
            }
        );
        let dev = HostSpecifier::parse("relay:10:BD:A3:B0:8E:30@http://127.0.0.1:2812/").unwrap();
        assert_eq!(
            dev,
            HostSpecifier::Relay {
                board,
                origin: "http://127.0.0.1:2812".to_string()
            }
        );
        assert_eq!(dev.to_string(), "relay:10bda3b08e30@http://127.0.0.1:2812");
        assert_eq!(
            HostSpecifier::parse("relay:10bda3b08e30")
                .unwrap()
                .to_string(),
            "relay:10bda3b08e30"
        );
        for bad in ["relay:", "relay:xyz", "relay:10bda3b08e30@ftp://x"] {
            let error = HostSpecifier::parse(bad).unwrap_err().to_string();
            assert!(error.contains("relay:"), "{bad}: {error}");
        }
    }

    #[test]
    fn the_invalid_specifier_error_lists_lan() {
        let error = HostSpecifier::parse("bogus").unwrap_err().to_string();
        assert!(error.contains("lan:"), "{error}");
    }

    #[test]
    fn test_display_emulator() {
        let spec = HostSpecifier::Emulator;
        assert_eq!(spec.to_string(), "emu");
    }
}

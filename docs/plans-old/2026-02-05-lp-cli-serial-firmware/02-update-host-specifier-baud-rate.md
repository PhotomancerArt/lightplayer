# Phase 2: Update HostSpecifier for Baud Rate

## Scope of phase

Update `HostSpecifier::Serial` to support baud rate configuration via query string syntax. Parse `serial:/dev/cu.X?baud=115200` and default to 115200 if not specified.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update HostSpecifier Enum

**File**: `lp-core/lp-client/src/specifier.rs`

Update `Serial` variant to include `baud_rate`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSpecifier {
    /// WebSocket connection
    WebSocket { url: String },
    /// Serial connection
    Serial { 
        port: Option<String>,  // None = auto-detect
        baud_rate: Option<u32>, // None = default to 115200
    },
    /// Local in-memory server
    Local,
    /// Emulator-based serial transport
    Emulator,
}
```

### 2. Update Parse Method

Update `parse()` method to handle query strings:

```rust
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
```

Add helper function to parse baud rate:

```rust
/// Parse baud rate from query string
///
/// Supports format: `baud=115200`
/// Returns None if baud parameter not found or invalid
fn parse_baud_rate_from_query(query: &str) -> Result<Option<u32>, anyhow::Error> {
    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            if key.trim() == "baud" {
                let baud = value.trim().parse::<u32>()
                    .map_err(|e| anyhow::anyhow!("Invalid baud rate '{}': {}", value, e))?;
                return Ok(Some(baud));
            }
        }
    }
    Ok(None)
}
```

### 3. Update Display Implementation

Update `fmt::Display` implementation:

```rust
impl fmt::Display for HostSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostSpecifier::WebSocket { url } => write!(f, "{url}"),
            HostSpecifier::Serial { port: None, baud_rate: None } => write!(f, "serial:auto"),
            HostSpecifier::Serial { port: None, baud_rate: Some(baud) } => {
                write!(f, "serial:auto?baud={}", baud)
            }
            HostSpecifier::Serial { port: Some(port), baud_rate: None } => {
                write!(f, "serial:{}", port)
            }
            HostSpecifier::Serial { port: Some(port), baud_rate: Some(baud) } => {
                write!(f, "serial:{}?baud={}", port, baud)
            }
            HostSpecifier::Local => write!(f, "local"),
            HostSpecifier::Emulator => write!(f, "emu"),
        }
    }
}
```

### 4. Add Helper Method

Add helper method to get baud rate with default:

```rust
impl HostSpecifier {
    // ... existing methods ...
    
    /// Get baud rate for serial connection, defaulting to 115200
    pub fn baud_rate(&self) -> u32 {
        match self {
            HostSpecifier::Serial { baud_rate, .. } => baud_rate.unwrap_or(115200),
            _ => 115200, // Default for non-serial (shouldn't be called)
        }
    }
}
```

### 5. Update Tests

**File**: `lp-core/lp-client/src/specifier.rs`

Add tests for query string parsing:

```rust
#[test]
fn test_parse_serial_with_baud_rate() {
    let spec = HostSpecifier::parse("serial:/dev/cu.usbmodem2101?baud=115200").unwrap();
    match spec {
        HostSpecifier::Serial { port: Some(p), baud_rate: Some(b) } => {
            assert_eq!(p, "/dev/cu.usbmodem2101");
            assert_eq!(b, 115200);
        }
        _ => panic!("Expected Serial with port and baud_rate"),
    }
}

#[test]
fn test_parse_serial_auto_with_baud_rate() {
    let spec = HostSpecifier::parse("serial:auto?baud=9600").unwrap();
    match spec {
        HostSpecifier::Serial { port: None, baud_rate: Some(b) } => {
            assert_eq!(b, 9600);
        }
        _ => panic!("Expected Serial with None port and baud_rate"),
    }
}

#[test]
fn test_parse_serial_default_baud_rate() {
    let spec = HostSpecifier::parse("serial:/dev/cu.usbmodem2101").unwrap();
    match spec {
        HostSpecifier::Serial { port: Some(_), baud_rate: None } => {}
        _ => panic!("Expected Serial with port and None baud_rate"),
    }
    assert_eq!(spec.baud_rate(), 115200); // Should default to 115200
}

#[test]
fn test_parse_serial_invalid_baud_rate() {
    let result = HostSpecifier::parse("serial:/dev/cu.usbmodem2101?baud=invalid");
    assert!(result.is_err());
}

#[test]
fn test_display_serial_with_baud_rate() {
    let spec = HostSpecifier::Serial {
        port: Some("/dev/cu.usbmodem2101".to_string()),
        baud_rate: Some(115200),
    };
    assert_eq!(spec.to_string(), "serial:/dev/cu.usbmodem2101?baud=115200");
}
```

## Validate

Run the following commands to validate the phase:

```bash
cd lp-core/lp-client
cargo check
cargo test
```

Fix any warnings or errors before proceeding.

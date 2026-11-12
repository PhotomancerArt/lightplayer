# Phase 3: Create Serial Port Detection Module

## Scope of phase

Create a module for detecting and selecting serial ports. Filter to only `/dev/cu.*` devices, use `dialoguer` for interactive selection when multiple ports are found, and parse baud rate from query strings.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Add Dependencies

**File**: `lp-cli/Cargo.toml`

Add dependencies:

```toml
[dependencies]
# ... existing dependencies ...
serialport = "5.4"
dialoguer = "0.11"
```

### 2. Create Serial Port Detection Module

**File**: `lp-cli/src/client/serial_port.rs` (NEW)

Create the module:

```rust
//! Serial port detection and selection
//!
//! Provides functions for detecting available serial ports and selecting
//! the appropriate port for ESP32 communication.

use anyhow::{Context, Result, bail};
use serialport::SerialPortInfo;
use std::collections::HashSet;

/// Serial port configuration
#[derive(Debug, Clone)]
pub struct SerialPortConfig {
    /// Port name (e.g., "/dev/cu.usbmodem2101")
    pub port: String,
    /// Baud rate
    pub baud_rate: u32,
}

/// Detect and select serial port
///
/// If `port` is Some, uses that port directly.
/// If `port` is None, auto-detects and prompts user if multiple found.
/// Parses `baud_rate` from query string or defaults to 115200.
///
/// # Arguments
///
/// * `port` - Optional port specification (e.g., "/dev/cu.usbmodem2101" or "auto")
/// * `baud_rate` - Optional baud rate (defaults to 115200)
///
/// # Returns
///
/// * `Ok(SerialPortConfig)` if port was selected successfully
/// * `Err` if detection failed or no ports found
pub fn detect_serial_port(
    port: Option<&str>,
    baud_rate: Option<u32>,
) -> Result<SerialPortConfig> {
    let baud_rate = baud_rate.unwrap_or(115200);

    if let Some(port_str) = port {
        // Manual port specification
        if port_str == "auto" || port_str.is_empty() {
            // Auto-detect
            auto_detect_port(baud_rate)
        } else {
            // Use specified port
            Ok(SerialPortConfig {
                port: port_str.to_string(),
                baud_rate,
            })
        }
    } else {
        // Auto-detect
        auto_detect_port(baud_rate)
    }
}

/// Auto-detect serial port
///
/// Lists all `/dev/cu.*` ports and prompts user if multiple found.
fn auto_detect_port(baud_rate: u32) -> Result<SerialPortConfig> {
    let ports = list_cu_ports()?;

    match ports.len() {
        0 => {
            bail!(
                "No serial ports found (looking for /dev/cu.* devices).\n\
                 Make sure your ESP32 is connected via USB."
            );
        }
        1 => {
            Ok(SerialPortConfig {
                port: ports[0].clone(),
                baud_rate,
            })
        }
        _ => {
            // Multiple ports - prompt user
            let selected = prompt_port_selection(&ports)?;
            Ok(SerialPortConfig {
                port: selected,
                baud_rate,
            })
        }
    }
}

/// List all `/dev/cu.*` ports
///
/// Filters to only callout devices (cu.*), ignoring terminal devices (tty.*).
fn list_cu_ports() -> Result<Vec<String>> {
    let all_ports = serialport::available_ports()
        .context("Failed to list serial ports")?;

    // Filter to only cu.* devices and collect unique base names
    let mut cu_ports: Vec<String> = all_ports
        .iter()
        .filter_map(|port_info| {
            let name = &port_info.port_name;
            if name.starts_with("/dev/cu.") {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    // Sort for consistent ordering
    cu_ports.sort();

    Ok(cu_ports)
}

/// Prompt user to select a port from multiple options
fn prompt_port_selection(ports: &[String]) -> Result<String> {
    use dialoguer::Select;

    println!("Multiple serial ports found:");
    for (i, port) in ports.iter().enumerate() {
        println!("  {}: {}", i + 1, port);
    }

    let selection = Select::new()
        .with_prompt("Select serial port")
        .items(ports)
        .default(0)
        .interact()
        .context("Failed to get user selection")?;

    Ok(ports[selection].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_serial_port_manual() {
        let config = detect_serial_port(Some("/dev/cu.usbmodem2101"), Some(115200)).unwrap();
        assert_eq!(config.port, "/dev/cu.usbmodem2101");
        assert_eq!(config.baud_rate, 115200);
    }

    #[test]
    fn test_detect_serial_port_default_baud() {
        let config = detect_serial_port(Some("/dev/cu.usbmodem2101"), None).unwrap();
        assert_eq!(config.baud_rate, 115200);
    }
}
```

### 3. Export Module

**File**: `lp-cli/src/client/mod.rs`

Add the module:

```rust
pub mod client_connect;
pub mod local_server;
pub mod serial_port;  // NEW
```

## Validate

Run the following commands to validate the phase:

```bash
cd lp-cli
cargo check
cargo test
```

Note: Tests that require actual serial ports may be skipped or mocked. The main validation is that the code compiles and basic logic tests pass.

Fix any warnings or errors before proceeding.

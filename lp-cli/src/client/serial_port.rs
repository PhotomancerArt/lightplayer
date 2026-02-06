//! Serial port detection and selection
//!
//! Provides functions for detecting available serial ports and selecting
//! the appropriate port for ESP32 communication.

use anyhow::{Context, Result, bail};
use lp_model::DEFAULT_SERIAL_BAUD_RATE;

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
/// Parses `baud_rate` from query string or defaults to DEFAULT_SERIAL_BAUD_RATE.
///
/// # Arguments
///
/// * `port` - Optional port specification (e.g., "/dev/cu.usbmodem2101" or "auto")
/// * `baud_rate` - Optional baud rate (defaults to DEFAULT_SERIAL_BAUD_RATE)
///
/// # Returns
///
/// * `Ok(SerialPortConfig)` if port was selected successfully
/// * `Err` if detection failed or no ports found
pub fn detect_serial_port(port: Option<&str>, baud_rate: Option<u32>) -> Result<SerialPortConfig> {
    let baud_rate = baud_rate.unwrap_or(DEFAULT_SERIAL_BAUD_RATE);

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
/// Lists all `/dev/cu.*` ports and intelligently selects USB serial ports.
/// If exactly one USB serial port is found, uses it automatically.
/// Otherwise prompts user if multiple USB serial ports or no USB serial ports found.
fn auto_detect_port(baud_rate: u32) -> Result<SerialPortConfig> {
    let all_ports = list_cu_ports()?;

    if all_ports.is_empty() {
        bail!(
            "No serial ports found (looking for /dev/cu.* devices).\n\
             Make sure your ESP32 is connected via USB."
        );
    }

    // Filter for USB serial ports (usbmodem, ttyUSB, etc.)
    let usb_ports: Vec<String> = all_ports
        .iter()
        .filter(|port| {
            port.contains("usbmodem")
                || port.contains("ttyUSB")
                || port.contains("ttyACM")
                || port.contains("tty.usbserial")
        })
        .cloned()
        .collect();

    match usb_ports.len() {
        0 => {
            // No USB serial ports found - prompt user to select from all ports
            eprintln!("No USB serial ports found. Available ports:");
            let selected = prompt_port_selection(&all_ports)?;
            Ok(SerialPortConfig {
                port: selected,
                baud_rate,
            })
        }
        1 => {
            // Exactly one USB serial port - use it automatically
            Ok(SerialPortConfig {
                port: usb_ports[0].clone(),
                baud_rate,
            })
        }
        _ => {
            // Multiple USB serial ports - prompt user to select
            eprintln!("Multiple USB serial ports found:");
            let selected = prompt_port_selection(&usb_ports)?;
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
    let all_ports = serialport::available_ports().context("Failed to list serial ports")?;

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
    use std::io::{IsTerminal, stdin};

    // Check if we're in an interactive terminal (check stdin since dialoguer uses it)
    // Also check if we're in a test environment - tests should not prompt
    if cfg!(test) || !stdin().is_terminal() {
        bail!(
            "Multiple serial ports found but not in an interactive terminal.\n\
             Available ports:\n{}\n\
             Please specify a port explicitly (e.g., serial:/dev/cu.usbmodem2101)",
            ports
                .iter()
                .map(|p| format!("  - {p}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

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
        assert_eq!(config.baud_rate, DEFAULT_SERIAL_BAUD_RATE);
    }
}

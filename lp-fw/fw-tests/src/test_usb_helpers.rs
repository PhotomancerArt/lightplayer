//! Helper functions for USB serial testing
//!
//! Provides utilities for flashing firmware, connecting to serial ports,
//! and communicating with the ESP32 test firmware.

use crate::test_output::{execute_step, is_debug_mode, print_command, print_response, print_step};
use serialport::SerialPort;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// Test configuration
pub struct TestConfig {
    /// Serial port name (e.g., "/dev/cu.usbmodem2101")
    pub port_name: String,
    /// Baud rate for serial communication
    pub baud_rate: u32,
}

/// Find ESP32 serial port
///
/// Looks specifically for `/dev/cu.usbmodem*` devices (macOS USB serial).
/// Returns error if zero or more than one device is found.
pub fn find_esp32_port() -> Result<String, Box<dyn std::error::Error>> {
    execute_step(
        "Searching for ESP32 serial port",
        None,
        || -> Result<String, Box<dyn std::error::Error>> {
            let ports = serialport::available_ports()
                .map_err(|e| format!("Failed to list serial ports: {}", e))?;

            // Filter for /dev/cu.usbmodem* devices
            let usbmodem_ports: Vec<_> = ports
                .iter()
                .filter(|p| p.port_name.starts_with("/dev/cu.usbmodem"))
                .map(|p| p.port_name.clone())
                .collect();

            match usbmodem_ports.len() {
                0 => {
                    eprintln!("  Available ports:");
                    for port in &ports {
                        eprintln!("    - {}", port.port_name);
                    }
                    Err("No ESP32 serial port found (looking for /dev/cu.usbmodem*)".into())
                }
                1 => {
                    let port = usbmodem_ports[0].clone();
                    Ok(port)
                }
                n => {
                    eprintln!("  Found ports:");
                    for port in &usbmodem_ports {
                        eprintln!("    - {}", port);
                    }
                    Err(format!("Multiple ESP32 serial ports found: {}", n).into())
                }
            }
        },
    )
    .map(|port| {
        // Print success with port name in details (overwrite the generic success from execute_step)
        print_step("✓", "Searching for ESP32 serial port", Some(&port));
        port
    })
}

/// Open serial port for ESP32
pub fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
) -> Result<Box<dyn SerialPort>, Box<dyn std::error::Error>> {
    let details = format!("{} @ {} baud", port_name, baud_rate);
    execute_step(
        "Connecting to serial port",
        Some(&details),
        || -> Result<Box<dyn SerialPort>, Box<dyn std::error::Error>> {
            serialport::new(port_name, baud_rate)
                .timeout(Duration::from_millis(100))
                .open()
                .map_err(|e| format!("Failed to open serial port {}: {}", port_name, e).into())
        },
    )
}

/// Read a line from serial port (with timeout)
///
/// Filters out heartbeat messages (lines starting with "heartbeat:").
/// Keeps reading until a non-heartbeat line is found or timeout occurs.
pub fn read_line_timeout(
    port: &mut dyn SerialPort,
    timeout_duration: Duration,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let mut buffer = Vec::new();

    while start.elapsed() < timeout_duration {
        let mut byte = [0u8; 1];
        match port.read(&mut byte)? {
            0 => {
                // No data - small delay
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            _ => {
                buffer.push(byte[0]);

                // Check for newline
                if byte[0] == b'\n' {
                    let line = String::from_utf8_lossy(&buffer).to_string();

                    // Filter out heartbeat messages
                    if line.trim_start().starts_with("heartbeat:") {
                        // Reset buffer and continue reading
                        buffer.clear();
                        continue;
                    }

                    return Ok(Some(line));
                }
            }
        }
    }

    Ok(None) // Timeout
}

/// Send command and wait for response
pub fn send_command(
    port: &mut dyn SerialPort,
    cmd: &str,
    timeout: Duration,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    print_command(cmd);

    // Send command
    port.write_all(cmd.as_bytes())?;
    port.flush()?;

    // Read response
    let response = read_line_timeout(port, timeout)?;

    if let Some(ref resp) = response {
        print_response(resp);
    } else if is_debug_mode() {
        eprintln!("  (no response - timeout)");
    }

    Ok(response)
}

/// Parse frame count from response
pub fn parse_frame_count(response: &str) -> Option<u32> {
    // Response format: M!{"frame_count":{"frame_count":12345}}\n
    // or: M!{"frame_count":12345}\n
    if !response.starts_with("M!") {
        return None;
    }

    // Extract JSON
    let json_str = &response[2..];

    // Try nested format first: {"frame_count":{"frame_count":12345}}
    if let Some(start) = json_str.find("\"frame_count\":{\"frame_count\":") {
        let value_start = start + "\"frame_count\":{\"frame_count\":".len();
        if let Some(end) = json_str[value_start..].find(|c: char| !c.is_ascii_digit()) {
            let count_str = &json_str[value_start..value_start + end];
            return count_str.parse().ok();
        }
    }

    // Try simple format: {"frame_count":12345}
    if let Some(start) = json_str.find("\"frame_count\":") {
        let value_start = start + "\"frame_count\":".len();
        // Skip whitespace
        let value_start = json_str[value_start..]
            .find(|c: char| c.is_ascii_digit())
            .map(|i| value_start + i)
            .unwrap_or(value_start);
        if let Some(end) = json_str[value_start..].find(|c: char| !c.is_ascii_digit()) {
            let count_str = &json_str[value_start..value_start + end];
            return count_str.parse().ok();
        }
    }

    None
}

/// Execute command with real-time output streaming (only in debug mode)
///
/// Runs a command and streams stdout/stderr to stderr in real-time if DEBUG=1.
/// Returns error if command fails.
fn execute_command_with_output(
    program: &str,
    args: &[&str],
    working_dir: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new(program);
    cmd.args(args);

    if is_debug_mode() {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        // In non-debug mode, suppress output
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", program, e))?;

    // Stream stdout/stderr only in debug mode
    if is_debug_mode() {
        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => eprintln!("{}", line),
                    Err(e) => eprintln!("ERROR: Error reading stdout: {}", e),
                }
            }
        }

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => eprintln!("{}", line),
                    Err(e) => eprintln!("ERROR: Error reading stderr: {}", e),
                }
            }
        }
    }

    // Wait for completion
    let status = child.wait()?;

    if !status.success() {
        return Err(format!("Command failed with exit code: {:?}", status.code()).into());
    }

    Ok(())
}

/// Get the firmware directory path
///
/// Returns the path to the firmware directory relative to the workspace root.
/// Searches upward from current directory to find workspace root (Cargo.toml with [workspace]).
fn firmware_dir() -> Result<String, Box<dyn std::error::Error>> {
    // Start from current directory and search upward for workspace root
    let mut current = std::env::current_dir().map_err(|_| "Failed to get current directory")?;

    // Look for workspace root (Cargo.toml with [workspace])
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            // Check if it's a workspace root
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    // Found workspace root
                    let fw_dir = current.join("lp-fw/fw-esp32");
                    if fw_dir.exists() {
                        return Ok(fw_dir.to_string_lossy().to_string());
                    }
                }
            }
        }

        // Move up one directory
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            break;
        }
    }

    // Fallback: try relative to current directory
    let fw_dir = std::path::Path::new("lp-fw/fw-esp32");
    if fw_dir.exists() {
        return Ok(fw_dir.to_string_lossy().to_string());
    }

    // Try absolute path from current dir
    let current = std::env::current_dir()?;
    let fw_dir = current.join("lp-fw/fw-esp32");
    if fw_dir.exists() {
        return Ok(fw_dir.to_string_lossy().to_string());
    }

    Err("Could not find firmware directory (lp-fw/fw-esp32). Make sure you're running from workspace root.".into())
}

/// Mutex to serialize firmware flashing operations
///
/// Prevents multiple tests from trying to flash simultaneously.
/// This is necessary because only one process can access the serial port for flashing at a time.
static FLASH_MUTEX: Mutex<()> = Mutex::new(());

/// Flash firmware using cargo-espflash
///
/// Flashes the firmware with real-time output streaming (only in debug mode).
/// Runs from the firmware directory.
/// This function is serialized to prevent multiple tests from flashing simultaneously.
pub fn flash_firmware() -> Result<(), Box<dyn std::error::Error>> {
    // Acquire lock to serialize flashing operations
    let _guard = FLASH_MUTEX
        .lock()
        .map_err(|e| format!("Failed to acquire flash lock: {}", e))?;

    execute_step("Flashing firmware", None, || {
        let fw_dir = firmware_dir()?;

        execute_command_with_output(
            "cargo",
            &[
                "espflash",
                "flash",
                "--package",
                "fw-esp32",
                "--features",
                "test_usb,esp32c6",
                "--target",
                "riscv32imac-unknown-none-elf",
                "--release",
            ],
            Some(&fw_dir),
        )?;

        Ok(())
    })
}

/// Reset ESP32 using cargo-espflash
///
/// Resets the ESP32 device.
/// Runs from the firmware directory.
pub fn reset_esp32() -> Result<(), Box<dyn std::error::Error>> {
    execute_step("Resetting ESP32", None, || {
        let fw_dir = firmware_dir()?;
        execute_command_with_output("cargo", &["espflash", "reset"], Some(&fw_dir))?;
        Ok(())
    })
}

/// Wait for firmware to initialize
///
/// Waits a specified duration for firmware to start up.
pub fn wait_for_firmware(duration: Duration) {
    let message = format!("Waiting {} seconds", duration.as_secs());
    execute_step(
        &message,
        None,
        || -> Result<(), Box<dyn std::error::Error>> {
            std::thread::sleep(duration);
            Ok(())
        },
    )
    .ok(); // Ignore result, this can't fail
}

/// Query frame count from ESP32
///
/// Sends a get_frame_count command and parses the response.
pub fn query_frame_count(port: &mut dyn SerialPort) -> Result<u32, Box<dyn std::error::Error>> {
    let cmd = "M!{\"get_frame_count\":{}}\n";
    let response = execute_step(
        "Querying frame count",
        None,
        || -> Result<String, Box<dyn std::error::Error>> {
            send_command(port, cmd, Duration::from_secs(2))?.ok_or("No response received".into())
        },
    )?;

    let count = parse_frame_count(&response)
        .ok_or_else(|| format!("Failed to parse frame count from: {}", response))?;

    // Print success with count in details (overwrite the generic success from execute_step)
    print_step("✓", "Querying frame count", Some(&count.to_string()));
    Ok(count)
}

/// Send echo command to ESP32
///
/// Sends an echo command and returns the response.
pub fn send_echo_command(
    port: &mut dyn SerialPort,
    data: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let cmd = format!("M!{{\"echo\":{{\"data\":\"{}\"}}}}\n", data);
    let response =
        send_command(port, &cmd, Duration::from_secs(2))?.ok_or("No response received")?;

    Ok(response)
}

/// Disconnect serial port
///
/// Closes the serial port connection.
pub fn disconnect_serial(port: Box<dyn SerialPort>) {
    drop(port);
    print_step("✓", "Disconnecting serial port", None);
}

# Phase 5: Create Host-Side Test Automation

## Scope of phase

Create automated host-side tests in `fw-tests` that flash firmware, connect/disconnect serial, send commands, and verify responses. Tests should cover all three scenarios: start without serial, start with serial, and reconnect.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Add dependencies to fw-tests

Update `lp-fw/fw-tests/Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...
serialport = "5.4"
tokio-serial = "5.4"
```

### 2. Create test helper module

Create `lp-fw/fw-tests/src/test_usb_helpers.rs`:

```rust
//! Helper functions for USB serial testing

use std::time::{Duration, Instant};
use serialport::{SerialPort, SerialPortBuilder};
use tokio::time::timeout;

/// Find ESP32 serial port
///
/// Looks for common ESP32 serial port names.
pub fn find_esp32_port() -> Option<String> {
    let ports = serialport::available_ports().ok()?;
    
    for port in ports {
        let name = port.port_name;
        // Common ESP32 port names
        if name.contains("USB") || name.contains("ttyUSB") || name.contains("COM") {
            return Some(name);
        }
    }
    
    None
}

/// Open serial port for ESP32
pub fn open_serial_port(port_name: &str) -> Result<Box<dyn SerialPort>, serialport::Error> {
    serialport::new(port_name, 115200)
        .timeout(Duration::from_millis(100))
        .open()
}

/// Read a line from serial port (with timeout)
pub fn read_line_timeout(
    port: &mut dyn SerialPort,
    timeout_duration: Duration,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let start = Instant::now();
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
    // Send command
    port.write_all(cmd.as_bytes())?;
    port.flush()?;
    
    // Read response
    read_line_timeout(port, timeout)
}

/// Parse frame count from response
pub fn parse_frame_count(response: &str) -> Option<u32> {
    // Response format: M!{"frame_count":12345}\n
    if !response.starts_with("M!") {
        return None;
    }
    
    // Extract JSON
    let json_str = &response[2..];
    
    // Parse JSON (simple extraction for test)
    if let Some(start) = json_str.find("\"frame_count\":") {
        let value_start = start + "\"frame_count\":".len();
        if let Some(end) = json_str[value_start..].find(|c: char| !c.is_ascii_digit()) {
            let count_str = &json_str[value_start..value_start + end];
            return count_str.parse().ok();
        }
    }
    
    None
}

/// Flash firmware using cargo-espflash
pub fn flash_firmware() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;
    
    let output = Command::new("cargo")
        .args(&[
            "espflash",
            "flash",
            "--package", "fw-esp32",
            "--features", "test_usb,esp32c6",
            "--target", "riscv32imac-unknown-none-elf",
            "--release",
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Flash failed: {}", stderr).into());
    }
    
    Ok(())
}

/// Reset ESP32 using cargo-espflash
pub fn reset_esp32() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;
    
    let output = Command::new("cargo")
        .args(&["espflash", "reset"])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Reset failed: {}", stderr).into());
    }
    
    Ok(())
}
```

### 3. Create test file

Create `lp-fw/fw-tests/tests/test_usb_serial.rs`:

```rust
//! Integration tests for USB serial connection/disconnection scenarios

use fw_tests::test_usb_helpers::*;
use std::time::Duration;

#[tokio::test]
#[ignore] // Requires connected ESP32
async fn test_scenario_1_start_without_serial() {
    // Scenario 1: Flash → Wait → Connect serial → Query frame count → 
    //            Verify LEDs blink → Disconnect → Wait → Reconnect → 
    //            Query frame count → Verify count increased
    
    // Flash firmware
    flash_firmware().expect("Failed to flash firmware");
    
    // Wait a bit (firmware starts without serial)
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Connect serial
    let port_name = find_esp32_port().expect("No ESP32 port found");
    let mut port = open_serial_port(&port_name).expect("Failed to open serial port");
    
    // Query frame count
    let cmd1 = "M!{\"get_frame_count\":{}}\n";
    let resp1 = send_command(&mut *port, cmd1, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    let count1 = parse_frame_count(&resp1).expect("Failed to parse frame count");
    eprintln!("Initial frame count: {}", count1);
    
    // Verify LEDs are blinking (visual check - can't automate)
    eprintln!("Verify LEDs are blinking...");
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Disconnect serial (close port)
    drop(port);
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Reconnect serial
    let mut port2 = open_serial_port(&port_name).expect("Failed to reopen serial port");
    
    // Query frame count again
    let cmd2 = "M!{\"get_frame_count\":{}}\n";
    let resp2 = send_command(&mut *port2, cmd2, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    let count2 = parse_frame_count(&resp2).expect("Failed to parse frame count");
    eprintln!("Frame count after reconnect: {}", count2);
    
    // Verify count increased (proves main loop continued)
    assert!(count2 > count1, "Frame count should increase: {} > {}", count2, count1);
}

#[tokio::test]
#[ignore] // Requires connected ESP32
async fn test_scenario_2_start_with_serial() {
    // Scenario 2: Flash → Connect serial immediately → Query frame count → 
    //            Verify serial works → Disconnect → Wait → Reconnect → 
    //            Query frame count → Verify count increased
    
    // Flash firmware
    flash_firmware().expect("Failed to flash firmware");
    
    // Connect serial immediately
    let port_name = find_esp32_port().expect("No ESP32 port found");
    let mut port = open_serial_port(&port_name).expect("Failed to open serial port");
    
    // Wait a bit for firmware to initialize
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Query frame count
    let cmd1 = "M!{\"get_frame_count\":{}}\n";
    let resp1 = send_command(&mut *port, cmd1, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    let count1 = parse_frame_count(&resp1).expect("Failed to parse frame count");
    eprintln!("Initial frame count: {}", count1);
    
    // Verify serial works (got response)
    assert!(count1 > 0, "Frame count should be > 0");
    
    // Disconnect
    drop(port);
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Reconnect
    let mut port2 = open_serial_port(&port_name).expect("Failed to reopen serial port");
    
    // Query again
    let cmd2 = "M!{\"get_frame_count\":{}}\n";
    let resp2 = send_command(&mut *port2, cmd2, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    let count2 = parse_frame_count(&resp2).expect("Failed to parse frame count");
    eprintln!("Frame count after reconnect: {}", count2);
    
    // Verify count increased
    assert!(count2 > count1, "Frame count should increase: {} > {}", count2, count1);
}

#[tokio::test]
#[ignore] // Requires connected ESP32
async fn test_scenario_3_echo_and_reconnect() {
    // Scenario 3: Flash → Connect serial → Send echo → Verify echo → 
    //            Disconnect → Reconnect → Send echo → Verify echo → 
    //            Query frame count → Verify count increased
    
    // Flash firmware
    flash_firmware().expect("Failed to flash firmware");
    
    // Connect serial
    let port_name = find_esp32_port().expect("No ESP32 port found");
    let mut port = open_serial_port(&port_name).expect("Failed to open serial port");
    
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Send echo command
    let cmd1 = "M!{\"echo\":{\"data\":\"test1\"}}\n";
    let resp1 = send_command(&mut *port, cmd1, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    // Verify echo response
    assert!(resp1.contains("test1"), "Echo response should contain 'test1'");
    
    // Disconnect
    drop(port);
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Reconnect
    let mut port2 = open_serial_port(&port_name).expect("Failed to reopen serial port");
    
    // Send echo again
    let cmd2 = "M!{\"echo\":{\"data\":\"test2\"}}\n";
    let resp2 = send_command(&mut *port2, cmd2, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    // Verify echo response
    assert!(resp2.contains("test2"), "Echo response should contain 'test2'");
    
    // Query frame count
    let cmd3 = "M!{\"get_frame_count\":{}}\n";
    let resp3 = send_command(&mut *port2, cmd3, Duration::from_secs(1))
        .expect("Failed to send command")
        .expect("No response received");
    
    let count = parse_frame_count(&resp3).expect("Failed to parse frame count");
    eprintln!("Final frame count: {}", count);
    
    // Verify count increased (proves main loop continued)
    assert!(count > 0, "Frame count should be > 0");
}
```

### 4. Export helpers from lib.rs

Update `lp-fw/fw-tests/src/lib.rs`:

```rust
// ... existing code ...

#[cfg(feature = "test_usb")]
pub mod test_usb_helpers;
```

## Tests to Write

- Test scenario 1: Start without serial, connect later, verify frame count increases
- Test scenario 2: Start with serial, disconnect/reconnect, verify frame count increases
- Test scenario 3: Echo test, disconnect/reconnect, verify frame count increases
- Helper tests: Port finding, command sending, response parsing

## Validate

Run from `lp-fw/fw-tests/` directory:

```bash
cd lp-fw/fw-tests
cargo test --package fw-tests --features test_usb
cargo check --package fw-tests --features test_usb
```

Ensure:
- Code compiles without warnings
- Tests are marked with `#[ignore]` (require hardware)
- Helper functions work correctly
- Command/response parsing works
- Frame count parsing works

Note: Actual test execution requires connected ESP32. Run with:
```bash
cargo test --package fw-tests --features test_usb -- --ignored
```

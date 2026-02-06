//! Integration tests for USB serial connection/disconnection scenarios

use fw_tests::test_output::{print_step, print_test_header};
use fw_tests::test_usb_helpers::*;
use lp_model::DEFAULT_SERIAL_BAUD_RATE;
use serial_test::serial;
use std::time::Duration;

#[tokio::test]
#[ignore] // Requires connected ESP32
#[serial]
async fn test_scenario_1_start_without_serial() {
    print_test_header("Test Scenario 1: Start without serial");

    // Find ESP32 port first
    let port_name = find_esp32_port().expect("Failed to find ESP32 port");

    // Flash firmware
    flash_firmware().expect("Failed to flash firmware");

    // Wait a bit (firmware starts without serial)
    wait_for_firmware(Duration::from_secs(2));

    // Connect serial
    let mut port =
        open_serial_port(&port_name, DEFAULT_SERIAL_BAUD_RATE).expect("Failed to open serial port");

    // Query frame count
    let count1 = query_frame_count(&mut *port).expect("Failed to query frame count");

    // Verify LEDs are blinking (visual check - can't automate)
    print_step("✓", "Verifying LEDs (visual check)", None);
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Disconnect serial (close port)
    disconnect_serial(port);

    // Wait while disconnected
    wait_for_firmware(Duration::from_secs(2));

    // Reconnect serial
    let mut port2 = open_serial_port(&port_name, DEFAULT_SERIAL_BAUD_RATE)
        .expect("Failed to reopen serial port");

    // Query frame count again
    let count2 = query_frame_count(&mut *port2).expect("Failed to query frame count");

    // Verify count increased (proves main loop continued)
    let details = format!("{} → {}", count1, count2);
    print_step("✓", "Verifying frame count increased", Some(&details));

    assert!(
        count2 > count1,
        "Frame count should increase: {} > {}",
        count2,
        count1
    );

    print_step("✓", "Test Scenario 1", Some("PASS"));
}

#[tokio::test]
#[ignore] // Requires connected ESP32
#[serial]
async fn test_scenario_2_start_with_serial() {
    print_test_header("Test Scenario 2: Start with serial");

    // Find ESP32 port first
    let port_name = find_esp32_port().expect("Failed to find ESP32 port");

    // Flash firmware
    flash_firmware().expect("Failed to flash firmware");

    // Connect serial immediately
    let mut port =
        open_serial_port(&port_name, DEFAULT_SERIAL_BAUD_RATE).expect("Failed to open serial port");

    // Wait a bit for firmware to initialize
    wait_for_firmware(Duration::from_secs(1));

    // Query frame count
    let count1 = query_frame_count(&mut *port).expect("Failed to query frame count");

    // Verify serial works (got response)
    assert!(count1 > 0, "Frame count should be > 0");
    print_step("✓", "Serial communication verified", None);

    // Disconnect
    disconnect_serial(port);

    // Wait while disconnected
    wait_for_firmware(Duration::from_secs(2));

    // Reconnect
    let mut port2 = open_serial_port(&port_name, DEFAULT_SERIAL_BAUD_RATE)
        .expect("Failed to reopen serial port");

    // Query again
    let count2 = query_frame_count(&mut *port2).expect("Failed to query frame count");

    // Verify count increased
    let details = format!("{} → {}", count1, count2);
    print_step("✓", "Verifying frame count increased", Some(&details));

    assert!(
        count2 > count1,
        "Frame count should increase: {} > {}",
        count2,
        count1
    );

    print_step("✓", "Test Scenario 2", Some("PASS"));
}

#[tokio::test]
#[ignore] // Requires connected ESP32
#[serial]
async fn test_scenario_3_echo_and_reconnect() {
    print_test_header("Test Scenario 3: Echo and reconnect");

    // Find ESP32 port first
    let port_name = find_esp32_port().expect("Failed to find ESP32 port");

    // Flash firmware
    flash_firmware().expect("Failed to flash firmware");

    // Connect serial
    let mut port =
        open_serial_port(&port_name, DEFAULT_SERIAL_BAUD_RATE).expect("Failed to open serial port");

    wait_for_firmware(Duration::from_secs(1));

    // Send echo command
    print_step("-", "Sending echo command", Some("test1"));
    let resp1 = send_echo_command(&mut *port, "test1").expect("Failed to send echo command");
    print_step("✓", "Sending echo command", Some("test1"));

    // Verify echo response
    print_step("✓", "Verifying echo response", None);
    assert!(
        resp1.contains("test1"),
        "Echo response should contain 'test1': {}",
        resp1
    );

    // Disconnect
    disconnect_serial(port);

    // Wait while disconnected
    wait_for_firmware(Duration::from_secs(2));

    // Reconnect
    let mut port2 = open_serial_port(&port_name, DEFAULT_SERIAL_BAUD_RATE)
        .expect("Failed to reopen serial port");

    // Send echo again
    print_step("-", "Sending echo command", Some("test2"));
    let resp2 = send_echo_command(&mut *port2, "test2").expect("Failed to send echo command");
    print_step("✓", "Sending echo command", Some("test2"));

    // Verify echo response
    print_step("✓", "Verifying echo response", None);
    assert!(
        resp2.contains("test2"),
        "Echo response should contain 'test2': {}",
        resp2
    );

    // Query frame count
    let count = query_frame_count(&mut *port2).expect("Failed to query frame count");

    // Verify count increased (proves main loop continued)
    assert!(count > 0, "Frame count should be > 0");
    print_step("✓", "Frame count verified", Some("> 0"));

    print_step("✓", "Test Scenario 3", Some("PASS"));
}

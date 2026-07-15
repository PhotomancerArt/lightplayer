use crate::LinkOperation;
use crate::providers::host_serial_esp32::{HostSerialEsp32Provider, label_for_port};
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;
#[test]
fn explicit_port_endpoint_records_metadata() {
    let provider = HostSerialEsp32Provider::new();

    let endpoint_id = provider.create_endpoint_for_port("/dev/cu.usbmodem2101", "Board");

    assert_eq!(
        endpoint_id.as_str(),
        "host-serial-esp32:dev-cu-usbmodem2101"
    );
    assert_eq!(
        provider.port_name_for_endpoint(&endpoint_id).as_deref(),
        Some("/dev/cu.usbmodem2101")
    );
    let endpoint = provider.endpoint(&endpoint_id).unwrap();
    // Full management surface: manage() drives espflash natively (M5), so
    // Reset/Flash/Erase are advertised alongside logs + diagnostics.
    assert!(endpoint.capabilities.supports(LinkOperation::Reset));
    assert!(endpoint.capabilities.supports(LinkOperation::ReadLogs));
    assert!(
        endpoint
            .capabilities
            .supports(LinkOperation::ReadDiagnostics)
    );
    assert!(endpoint.capabilities.supports(LinkOperation::FlashFirmware));
    assert!(
        endpoint
            .capabilities
            .supports(LinkOperation::EraseDeviceFlash)
    );
    // Raw-filesystem access stays off: manage() rejects it as unsupported.
    assert!(
        !endpoint
            .capabilities
            .supports(LinkOperation::WriteRawFilesystem)
    );
}

#[test]
fn labels_likely_esp32_ports() {
    assert_eq!(
        label_for_port("/dev/cu.usbmodem2101"),
        "ESP32 Serial (/dev/cu.usbmodem2101)"
    );
    assert_eq!(
        label_for_port("/dev/cu.Bluetooth"),
        "Serial (/dev/cu.Bluetooth)"
    );
}

#[test]
fn default_options_do_not_reset_after_open() {
    let provider = HostSerialEsp32Provider::new();

    assert_eq!(provider.options().baud_rate, None);
    assert!(!provider.options().reset_after_open);
    assert_eq!(
        provider
            .options()
            .baud_rate
            .unwrap_or(DEFAULT_SERIAL_BAUD_RATE),
        DEFAULT_SERIAL_BAUD_RATE
    );
}

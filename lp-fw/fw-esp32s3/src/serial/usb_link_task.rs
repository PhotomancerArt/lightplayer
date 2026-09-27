//! The S3's USB host link task: USB-Serial-JTAG, owned by one embassy task
//! that runs lp-link over it (plan `lp-link-usb-cutover`, P2). The C6's twin
//! (`fw-esp32c6/src/serial/usb_link_task.rs`) minus its spike and fixture
//! variants.
//!
//! Everything the link does is chip-free and lives in
//! [`fw_esp32_common::usb_link`]; what stays here is the chip: the peripheral
//! split into async halves, the TX half behind the IN-endpoint gate
//! ([`fw_esp32_common::serial::in_endpoint`]), the SOF-reading cable monitor,
//! and the watchdog's liveness tick.

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_esp32_common::serial::in_endpoint::{InEndpoint, InEndpointRegs};
use fw_esp32_common::usb_link::{UsbLinkChip, UsbLinkShared, run_usb_link};

use crate::board::esp32s3::usb_connection::{UsbConnectionMonitor, UsbSerialJtagInEndpoint};

/// The host link, for the life of the boot.
#[embassy_executor::task]
pub async fn usb_link_task(
    usb_device: esp_hal::peripherals::USB_DEVICE<'static>,
    shared: &'static UsbLinkShared,
) {
    let (rx, tx) = UsbSerialJtag::new(usb_device).into_async().split();
    // esp-println shares the IN endpoint; the gate waits for it to be free
    // and clears the stale `serial_in_empty` before every packet.
    let tx = InEndpoint::<_, UsbSerialJtagInEndpoint>::new(tx);
    let chip = S3UsbChip {
        monitor: UsbConnectionMonitor::new(),
    };
    run_usb_link(rx, tx, shared, chip).await
}

/// The S3's register facts for the link loop.
struct S3UsbChip {
    monitor: UsbConnectionMonitor,
}

impl UsbLinkChip for S3UsbChip {
    fn note_io_alive(&mut self) {
        crate::recovery::watchdog::note_io_alive();
    }

    fn host_enumerated(&mut self) -> bool {
        self.monitor.poll();
        self.monitor.is_enumerated()
    }

    fn in_ep_free(&self) -> bool {
        UsbSerialJtagInEndpoint::in_ep_free()
    }

    fn reset(&mut self) -> ! {
        esp_hal::system::software_reset()
    }
}

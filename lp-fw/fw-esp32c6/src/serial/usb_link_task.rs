//! The C6's USB host link task: USB-Serial-JTAG, owned by one embassy task
//! that runs lp-link over it (plan `lp-link-usb-cutover`, P2).
//!
//! Everything the link does is chip-free and lives in
//! [`fw_esp32_common::usb_link`]; what stays here is the chip: the peripheral
//! split into async halves, the TX half behind the IN-endpoint gate
//! ([`fw_esp32_common::serial::in_endpoint`], so esp-println's raw boot and
//! panic text never lands inside a frame's packet), the SOF-reading cable
//! monitor, and the watchdog's liveness tick.
//!
//! Two non-shipped variants keep compiling: `spike_uart0_link` (the same link
//! over UART0, for Espressif's binary emulator) and
//! `fixture-no-in-endpoint-gate` (esp-hal's TX half as it comes).

use fw_esp32_common::usb_link::{UsbLinkChip, UsbLinkShared, run_usb_link};

use crate::board::esp32c6::usb_connection::UsbConnectionMonitor;
#[cfg(not(feature = "spike_uart0_link"))]
use crate::board::esp32c6::usb_connection::UsbSerialJtagInEndpoint;
#[cfg(not(feature = "spike_uart0_link"))]
use esp_hal::usb_serial_jtag::UsbSerialJtag;
#[cfg(not(any(feature = "spike_uart0_link", feature = "fixture-no-in-endpoint-gate")))]
use fw_esp32_common::serial::in_endpoint::InEndpoint;
#[cfg(not(feature = "spike_uart0_link"))]
use fw_esp32_common::serial::in_endpoint::InEndpointRegs;

/// The host link, for the life of the boot.
#[embassy_executor::task]
pub async fn usb_link_task(
    usb_device: esp_hal::peripherals::USB_DEVICE<'static>,
    shared: &'static UsbLinkShared,
) {
    #[cfg(not(feature = "spike_uart0_link"))]
    let (rx, tx) = {
        let (rx, tx) = UsbSerialJtag::new(usb_device).into_async().split();
        // esp-println shares the IN endpoint and esp-hal's writer neither
        // checks it is free nor clears a stale `serial_in_empty`; the gate
        // does both before every packet (PR #805,
        // docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md).
        #[cfg(not(feature = "fixture-no-in-endpoint-gate"))]
        let tx = InEndpoint::<_, UsbSerialJtagInEndpoint>::new(tx);
        (rx, tx)
    };
    // esp-emu spike: the same link over UART0; everything after this point
    // is generic over `embedded_io_async::{Read, Write}`.
    #[cfg(feature = "spike_uart0_link")]
    let (rx, tx) = {
        let _ = usb_device;
        crate::serial::spike_uart0::link().split()
    };

    let chip = C6UsbChip {
        monitor: UsbConnectionMonitor::new(),
    };
    run_usb_link(rx, tx, shared, chip).await
}

/// The C6's register facts for the link loop.
struct C6UsbChip {
    monitor: UsbConnectionMonitor,
}

impl UsbLinkChip for C6UsbChip {
    fn note_io_alive(&mut self) {
        crate::recovery::watchdog::note_io_alive();
    }

    fn host_enumerated(&mut self) -> bool {
        self.monitor.poll();
        self.monitor.is_enumerated()
    }

    fn in_ep_free(&self) -> bool {
        #[cfg(not(feature = "spike_uart0_link"))]
        return UsbSerialJtagInEndpoint::in_ep_free();
        #[cfg(feature = "spike_uart0_link")]
        return true;
    }
}

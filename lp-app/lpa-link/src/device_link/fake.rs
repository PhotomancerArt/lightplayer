//! The fake [`Link`](lpa_devices::link::Link): the model's host test vehicle.
//!
//! A [`FakeEsp32Device`] is already a byte-level fake — ROM boot output for
//! blank flash / download mode / foreign firmware, and for the LightPlayer
//! state a REAL host `LpServer` behind REAL `M!` framing, with failure
//! injection on the stream. Handed to [`ByteStreamLink`], it becomes a
//! [`Link`](lpa_devices::link::Link), so a host test drives the whole model
//! through the same demux, the same frame mapping and the same command
//! execution the browser build uses.
//!
//! That is the point: every device bug so far lived BELOW the record level —
//! framing, boot-output classification, timing — and a fake that answered at
//! the model's own vocabulary would hide exactly those.

use lpa_devices::link::LinkInfo;

use crate::device_link::byte_stream::ByteStreamLink;
use crate::providers::fake_device::{FakeDeviceByteStream, FakeEsp32Device};

/// One [`Link`](lpa_devices::link::Link) over a scripted fake device.
pub type FakeDeviceLink = ByteStreamLink<FakeDeviceByteStream>;

/// Attach a link to a fake device. The device is shared (clone the handle to
/// keep scripting it mid-test: `set_drop_responses`, `fake_flash`, …), and it
/// outlives the link — a reconnect is a new link on the same board.
pub fn fake_device_link(info: LinkInfo, device: &FakeEsp32Device) -> FakeDeviceLink {
    ByteStreamLink::new(info, FakeDeviceByteStream::new(device.clone()))
}

/// A plausible [`LinkInfo`] for a fake board on a fake port.
///
/// `endpoint` is what the model's weakest identity rung binds to, so tests
/// that care about routing (two boards, a replug) pass distinct keys.
pub fn fake_link_info(endpoint: &str) -> LinkInfo {
    LinkInfo {
        label: format!("Fake ESP32 ({endpoint})"),
        endpoint: lpa_devices::identity::EndpointKey(endpoint.to_string()),
        usb: Some(lpa_devices::link::UsbIds {
            // Espressif native USB: the pair a C6 dev board enumerates as.
            vendor: 0x303a,
            product: 0x1001,
        }),
        serial_number: None,
    }
}

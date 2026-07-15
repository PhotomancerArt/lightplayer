//! The concrete wire a device session speaks the app protocol over.
//!
//! Two shapes exist:
//!
//! - **Host transport** (`device-session-host`): the link connection carries
//!   a [`LinkServerConnection`] — the real `M!` serial framing behind a
//!   shared transport. Frames are sent/received through it; observed serial
//!   LINES are a tap copy surfaced by the provider (`take_lines`), so `M!`
//!   lines seen there are duplicates and get dropped.
//! - **Browser lines** (`browser-serial-esp32` on wasm): there is no host
//!   transport — the JS serial controller splits the byte stream into whole
//!   lines, and `M!` lines ARE the protocol frames. The session decodes them
//!   into this pending queue during its line pump; sends go out as
//!   `M!{json}\n` writes on the provider.
//!
//! Feature combinations with neither wire (e.g. a plain `device-session`
//! build with no provider feature) leave this enum uninhabited; such builds
//! also have no connector that could open a hardware link, and
//! `open_device_link` fails before ever constructing one.

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
use std::collections::VecDeque;

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
use lpc_wire::WireServerMessage;

#[cfg(feature = "device-session-host")]
use crate::LinkServerConnection;

/// The app-protocol wire behind one link generation. Swapped whole by
/// rebuilds (`DeviceShared::rebuild_link`).
pub(super) enum DeviceWire {
    /// Host protocol transport (real serial framing / fake device / host
    /// process pipe).
    #[cfg(feature = "device-session-host")]
    Transport(LinkServerConnection),
    /// Browser Web Serial: whole lines from JS; decoded `M!` frames queue
    /// here until the channel (or the readiness engine) consumes them.
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    BrowserLines {
        pending: VecDeque<WireServerMessage>,
    },
}

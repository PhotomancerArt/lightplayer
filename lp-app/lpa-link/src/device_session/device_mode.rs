//! Mode-exclusive access to the device wire.
//!
//! A hardware device has ONE wire. The app protocol and management
//! operations (flash/erase/reset) must never interleave on it, so the
//! session gates its API on an exclusive [`DeviceMode`]: the app-protocol
//! channel errors cleanly while `Management` holds the wire, and `manage()`
//! refuses unless it can take the mode. The complementary direction is the
//! in-flight counter behind [`ChannelUseGuard`]: management also refuses
//! while an app-protocol request is mid-send/receive, so the wire is never
//! torn down under a request.

use std::cell::Cell;
use std::rc::Rc;

/// Who currently owns the device wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceMode {
    /// Normal operation: the app-protocol channel may speak (when `Ready`).
    AppProtocol,
    /// A management operation owns the wire; the app-protocol channel is
    /// invalidated until the mode is released.
    Management,
}

/// RAII release for a taken [`DeviceMode::Management`]: dropping the guard
/// returns the wire to [`DeviceMode::AppProtocol`].
pub struct DeviceModeGuard {
    mode: Rc<Cell<DeviceMode>>,
}

impl DeviceModeGuard {
    pub(crate) fn new(mode: Rc<Cell<DeviceMode>>) -> Self {
        Self { mode }
    }
}

impl Drop for DeviceModeGuard {
    fn drop(&mut self) {
        self.mode.set(DeviceMode::AppProtocol);
    }
}

/// RAII marker for one app-protocol request being inside a transport
/// send/receive. While the count is nonzero, management refuses to take the
/// wire (a `Cell` counter only — safe to hold across awaits).
pub(crate) struct ChannelUseGuard<'a> {
    in_flight: &'a Cell<u32>,
}

impl<'a> ChannelUseGuard<'a> {
    pub(crate) fn new(in_flight: &'a Cell<u32>) -> Self {
        in_flight.set(in_flight.get() + 1);
        Self { in_flight }
    }
}

impl Drop for ChannelUseGuard<'_> {
    fn drop(&mut self) {
        self.in_flight.set(self.in_flight.get().saturating_sub(1));
    }
}

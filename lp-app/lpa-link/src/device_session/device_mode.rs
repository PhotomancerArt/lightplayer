//! Mode-exclusive access to the device wire.
//!
//! A hardware device has ONE wire. The app protocol and management
//! operations (flash/erase/reset, P3) must never interleave on it, so the
//! session gates its API on an exclusive [`DeviceMode`]: the app-protocol
//! channel errors cleanly while `Management` holds the wire, and P3's
//! `manage()` will refuse unless it can take the mode. This file is the P2
//! scaffolding for that contract.

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

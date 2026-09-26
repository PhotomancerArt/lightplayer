//! Power-off as a node-facing service.
//!
//! A `PowerButton` node decides *that* the device should power off; it never
//! does it. Tearing the project down from inside its own tick would pull the
//! runtime out from under the frame, and entering deep sleep is a platform
//! act the engine has no business performing. So the node hands a
//! [`PowerOffRequest`] to a [`PowerService`], and whoever implements that
//! service — the server — finishes the frame, unloads the projects, and asks
//! the platform to sleep. See `docs/adr/2026-06-16-power-button-runtime-event.md`.

use alloc::string::String;
use core::fmt;

use lpc_model::HwEndpointSpec;

/// Pin level that wakes the device from deep sleep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerWakeLevel {
    /// Wake when the pin is pulled low (a button to ground).
    Low,
    /// Wake when the pin is driven high (a switch that means "on" when high).
    High,
}

/// A request to power the device off until `endpoint` reaches `wake_level`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerOffRequest {
    pub endpoint: HwEndpointSpec,
    pub wake_level: PowerWakeLevel,
}

/// Power-off service handed to runtime nodes.
pub trait PowerService {
    /// Check the wake source and queue the power-off.
    ///
    /// Returning `Ok` means the request is valid and will be carried out
    /// after the current frame; an `Err` means it never will be (for
    /// example, the endpoint cannot wake the chip), and nothing sleeps.
    fn request_power_off(&self, request: PowerOffRequest) -> Result<(), PowerError>;

    /// Whether a host is attached over the device's own link right now —
    /// Studio or a computer on the USB-Serial-JTAG port. A switch-mode power
    /// button stays awake while this is true, because deep sleep drops the
    /// link.
    fn host_attached(&self) -> bool;
}

/// Why a power-off cannot happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerError {
    message: String,
}

impl PowerError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for PowerError {}

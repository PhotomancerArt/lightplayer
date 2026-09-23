//! Server-owned power-off: nodes ask, the server finishes the frame and acts.
//!
//! A `PowerButton` node hands its [`PowerOffRequest`] to the engine's
//! [`PowerService`]. On a server that service is a [`PowerOffQueue`]: it
//! checks the wake source with the platform *now*, so a pin that cannot wake
//! the chip is refused before anything sleeps, and holds the request until
//! [`crate::LpServer::advance_frame`] has finished ticking. The server then
//! unloads every project — which closes their outputs, leaving the LEDs dark
//! — and calls [`PowerPlatform::enter_power_off`], which does not return on
//! hardware. See `docs/adr/2026-06-16-power-button-runtime-event.md`.

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use lpc_engine::{PowerError, PowerOffRequest, PowerService};

/// The embedder's power-off capability — deep sleep on an ESP32-C6.
///
/// The server is chip-agnostic and sans-IO, so it cannot sleep anything
/// itself; an embedder that can installs this via
/// [`crate::LpServer::set_power_platform`]. Embedders that cannot (host,
/// browser, emulator) leave it unset, and a power button there reports
/// "no power service" rather than pretending.
pub trait PowerPlatform {
    /// Whether `request` could be carried out: the endpoint is a pin this
    /// board can wake from, at a level it supports. Called when the node asks,
    /// before any project is torn down.
    fn check_power_off(&self, request: &PowerOffRequest) -> Result<(), PowerError>;

    /// Power off until the wake condition in `request`. Does not return on
    /// success on hardware (waking is a reset); returns only on failure — or,
    /// for a test double, to record the call.
    fn enter_power_off(&self, request: &PowerOffRequest) -> Result<(), PowerError>;

    /// Whether a host is attached over the device's own link right now.
    fn host_attached(&self) -> bool;
}

/// The [`PowerService`] a server hands its engines: validates with the
/// platform and holds the request until the frame is over.
pub struct PowerOffQueue {
    platform: Rc<dyn PowerPlatform>,
    pending: RefCell<Option<PowerOffRequest>>,
}

impl PowerOffQueue {
    pub fn new(platform: Rc<dyn PowerPlatform>) -> Self {
        Self {
            platform,
            pending: RefCell::new(None),
        }
    }

    pub fn platform(&self) -> &Rc<dyn PowerPlatform> {
        &self.platform
    }

    /// The request queued during the last frame, if any. Taking it clears it.
    pub fn take_pending(&self) -> Option<PowerOffRequest> {
        self.pending.borrow_mut().take()
    }
}

impl PowerService for PowerOffQueue {
    fn request_power_off(&self, request: PowerOffRequest) -> Result<(), PowerError> {
        self.platform.check_power_off(&request)?;
        log::info!(
            "power-off queued: endpoint={} wake={:?}",
            request.endpoint,
            request.wake_level
        );
        *self.pending.borrow_mut() = Some(request);
        Ok(())
    }

    fn host_attached(&self) -> bool {
        self.platform.host_attached()
    }
}

use alloc::boxed::Box;

use crate::OutputError;
use crate::{HardwareEndpointError, HwDriver, HwEndpoint, HwEndpointId};

/// Maximum LEDs a single WS281x output port will drive.
///
/// Requests beyond this are truncated by [`ws281x_capped_byte_count`]. The cap
/// is a per-port buffer/latency bound imposed by the RMT drivers, not a
/// protocol limit. This is deliberately the only definition — it used to be hand-copied
/// into two firmware crates and drifted silently
/// (docs/debt/output-channel-led-cap-silent-truncation.md).
///
/// Enforced once, identically, at the engine's output-flush seam
/// (`lpc-engine`'s `wire_slice`) so host, emulator, and device all grant the
/// same byte count for the same authored port; `Esp32OutputProvider` keeps
/// its own check on top as defense-in-depth, not as the source of truth.
pub const WS281X_MAX_LEDS_PER_PORT: usize = 1024;

/// Cap a requested frame's byte count to [`WS281X_MAX_LEDS_PER_PORT`].
///
/// Returns the granted byte count and whether the cap actually truncated the
/// request. Callers own logging: warn once per cap-crossing, never per frame.
pub fn ws281x_capped_byte_count(requested: u32) -> (u32, bool) {
    let max_byte_count = (WS281X_MAX_LEDS_PER_PORT * 3) as u32;
    let granted = requested.min(max_byte_count);
    (granted, granted < requested)
}

/// Configuration used when opening or resizing a WS281x endpoint.
///
/// `byte_count` is the number of protocol bytes in one output frame, normally
/// `led_count * 3` for RGB strips. Rendering concerns such as interpolation,
/// dithering, and white-point correction live above this hardware boundary.
#[derive(Debug, Clone)]
pub struct Ws281xConfig {
    byte_count: u32,
}

impl Ws281xConfig {
    /// Create a WS281x config for one frame of protocol bytes.
    pub fn new(byte_count: u32) -> Self {
        Self { byte_count }
    }

    /// Number of RGB protocol bytes in one frame.
    pub fn byte_count(&self) -> u32 {
        self.byte_count
    }
}

/// Opened WS281x hardware output.
///
/// Implementations receive already-rendered 8-bit protocol bytes. Callers that
/// start from 16-bit RGB samples should run display-pipeline processing before
/// writing here.
///
/// # Two transmission shapes
///
/// [`write`] is the blocking form: one frame in, complete on return. The
/// [`start`]/[`wait_complete`] pair is the split form that lets a caller run
/// several outputs **concurrently** — start every wire's frame, and pay the
/// wire time once instead of once per wire. The defaults make the split form a
/// synonym for `write`, so an implementation that only ever transmits
/// synchronously (virtual, emulator) implements nothing extra and a caller
/// using the split form is always correct against it.
///
/// [`write`]: Ws281xOutput::write
/// [`start`]: Ws281xOutput::start
/// [`wait_complete`]: Ws281xOutput::wait_complete
pub trait Ws281xOutput {
    /// Write one full raw RGB frame, blocking until it is on the wire.
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError>;

    /// Change the expected frame size for subsequent writes.
    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError>;

    /// Begin transmitting one full raw RGB frame without waiting for it.
    ///
    /// The default is [`Ws281xOutput::write`] — blocking, complete on return —
    /// so only genuinely asynchronous implementations override this.
    ///
    /// # Safety
    ///
    /// An overriding implementation may keep reading `data` (for example from
    /// an interrupt handler) after this returns. The caller must keep the
    /// referenced bytes **alive, in place, and unmodified** until the next
    /// [`Ws281xOutput::wait_complete`] on this output returns, or the output
    /// is dropped — an implementation's drop must stop the transmission before
    /// giving up the hardware.
    unsafe fn start(&mut self, data: &[u8]) -> Result<(), OutputError> {
        self.write(data)
    }

    /// Wait for the frame begun by [`Ws281xOutput::start`] to finish.
    ///
    /// A no-op when no frame is in flight — including always, for an
    /// implementation whose `start` is the blocking default. On error the
    /// output must be left idle (the frame aborted), so the caller may reuse
    /// or free the frame bytes either way.
    fn wait_complete(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    /// May a frame this output started keep transmitting while the render
    /// core does arbitrary work?
    ///
    /// `true` claims the transmission survives anything the caller's core
    /// does after `start` returns — interrupt masking included — so a
    /// provider may skip its end-of-frame barrier for this output and let
    /// wire time overlap the next render. The caller's frame-lifetime duty is
    /// unchanged: the bytes stay alive and unmodified until
    /// [`Ws281xOutput::wait_complete`], which now typically runs at the
    /// *next* write ("wait-before-stage") rather than at the frame barrier.
    ///
    /// The default is `false` — barrier semantics — and `false` must remain
    /// the default for every implementation that has not proven otherwise on
    /// its own silicon: the classic ESP32 measured ~99 % frame truncation
    /// when a wire transmitted under engine load with its ISR on the render
    /// core. The one current `true` is that same chip once its refill ISR is
    /// bound on the dedicated APP core.
    ///
    /// ⚠️ Any delegating wrapper around a `dyn Ws281xOutput` must forward
    /// this method explicitly. A wrapper that leaves the default inherits
    /// `false` silently — which is at least fail-safe — but the mirror-image
    /// trap on `OutputProvider::flush` (a wrapper silently resolving to a
    /// defaulted no-op) cost five flash cycles once; see the concurrent-flush
    /// ADR's near-miss note.
    fn background_tx_safe(&self) -> bool {
        false
    }
}

/// Driver that exposes WS281x-capable hardware endpoints.
pub trait Ws281xDriver: HwDriver {
    /// List currently known WS281x endpoints.
    fn endpoints(&self) -> alloc::vec::Vec<HwEndpoint>;

    /// Open one endpoint and claim its GPIO/timing resources.
    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_passes_through_at_or_under_the_limit() {
        let max = (WS281X_MAX_LEDS_PER_PORT * 3) as u32;
        assert_eq!(ws281x_capped_byte_count(3), (3, false));
        assert_eq!(ws281x_capped_byte_count(max), (max, false));
    }

    #[test]
    fn cap_truncates_and_reports_it_above_the_limit() {
        let max = (WS281X_MAX_LEDS_PER_PORT * 3) as u32;
        assert_eq!(ws281x_capped_byte_count(max + 3), (max, true));
        assert_eq!(ws281x_capped_byte_count(u32::MAX), (max, true));
    }
}

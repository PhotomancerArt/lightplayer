use alloc::boxed::Box;

use crate::OutputError;
use crate::{HardwareEndpointError, HwDriver, HwEndpoint, HwEndpointId};

/// Maximum LEDs a single WS281x output channel will drive.
///
/// Requests beyond this are truncated by [`ws281x_capped_byte_count`]. The cap
/// is a per-channel buffer/latency bound in the RMT drivers, not a protocol
/// limit. This is deliberately the only definition — it used to be hand-copied
/// into two firmware crates and drifted silently
/// (docs/debt/output-channel-led-cap-silent-truncation.md).
pub const WS281X_MAX_LEDS_PER_CHANNEL: usize = 256;

/// Cap a requested frame's byte count to [`WS281X_MAX_LEDS_PER_CHANNEL`].
///
/// Returns the granted byte count and whether the cap actually truncated the
/// request. Callers own logging: warn once per cap-crossing, never per frame.
pub fn ws281x_capped_byte_count(requested: u32) -> (u32, bool) {
    let max_byte_count = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
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
pub trait Ws281xOutput {
    /// Write one full raw RGB frame.
    fn write(&mut self, data: &[u8]) -> Result<(), OutputError>;

    /// Change the expected frame size for subsequent writes.
    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError>;
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
        let max = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
        assert_eq!(ws281x_capped_byte_count(3), (3, false));
        assert_eq!(ws281x_capped_byte_count(max), (max, false));
    }

    #[test]
    fn cap_truncates_and_reports_it_above_the_limit() {
        let max = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
        assert_eq!(ws281x_capped_byte_count(max + 3), (max, true));
        assert_eq!(ws281x_capped_byte_count(u32::MAX), (max, true));
    }
}

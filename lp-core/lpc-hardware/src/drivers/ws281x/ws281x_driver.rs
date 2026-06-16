use alloc::boxed::Box;

use crate::OutputError;
use crate::{HardwareEndpointError, HwDriver, HwEndpoint, HwEndpointId};

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

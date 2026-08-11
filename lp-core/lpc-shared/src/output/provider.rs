use crate::display_pipeline::DisplayPipelineOptions;
use lpc_hardware::HwEndpointSpec;
use lpc_hardware::OutputError;

/// Options for output driver (DisplayPipeline). Alias for DisplayPipelineOptions.
pub type OutputDriverOptions = DisplayPipelineOptions;

/// Handle for an opened output port
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputPortHandle(i32);

impl OutputPortHandle {
    /// Create a new output port handle
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    /// Get the underlying i32 value
    pub fn as_i32(&self) -> i32 {
        self.0
    }

    /// Check if this is an invalid handle (typically -1)
    pub fn is_invalid(&self) -> bool {
        self.0 < 0
    }
}

/// Output format/protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// WS2811/WS2812 RGB LED protocol
    Ws2811,
}

/// Trait for output providers (hardware drivers, test implementations, etc.)
pub trait OutputProvider {
    /// Open an output port
    ///
    /// # Arguments
    /// * `endpoint` - Authored hardware endpoint spec, such as `ws281x:local:D10`
    /// * `byte_count` - Total number of bytes to allocate for this port
    /// * `format` - Output format/protocol
    ///
    /// # Returns
    /// Returns `OutputPortHandle` on success, or `OutputError` if:
    /// - Endpoint is already open
    /// - Invalid parameters
    /// - Hardware initialization failed
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputPortHandle, OutputError>;

    /// Write 16-bit RGB data to an output port
    ///
    /// # Arguments
    /// * `handle` - Output port handle from `open()`
    /// * `data` - 16-bit RGB data: [r,g,b; num_leds], length = num_leds * 3
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or `OutputError` if:
    /// - Handle is invalid
    /// - Data length doesn't match expected (num_leds * 3)
    /// - Hardware write failed
    fn write(&self, handle: OutputPortHandle, data: &[u16]) -> Result<(), OutputError>;

    /// Close an output port
    ///
    /// # Arguments
    /// * `handle` - Output port handle from `open()`
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or `OutputError` if handle is invalid
    fn close(&self, handle: OutputPortHandle) -> Result<(), OutputError>;

    /// Complete every transmission begun by [`write`](OutputProvider::write).
    ///
    /// A provider may begin a hardware transmission in `write` and return
    /// without waiting for it, so ports written back to back transmit
    /// **concurrently** — the frame then pays for its slowest wire rather than
    /// the sum of all of them. Such a provider finishes the job here: wait out
    /// every in-flight port and report the first failure. The engine calls
    /// this once per frame, after the last `write` of the flush.
    ///
    /// The default is a no-op, correct for any provider whose `write` is
    /// already synchronous.
    fn flush(&self) -> Result<(), OutputError> {
        Ok(())
    }

    /// Change signal for endpoint availability.
    ///
    /// The value changes whenever an endpoint that refused to [`open`] might
    /// now accept — that is, whenever hardware ownership moved. Callers
    /// compare it for inequality only; nothing about ordering or magnitude is
    /// promised.
    ///
    /// This exists so a caller holding a failed endpoint can wait instead of
    /// asking: re-attempting an open costs a full enumeration of the board,
    /// and a sink whose pin is simply not there would pay that on every frame
    /// forever.
    ///
    /// The default answers "this hardware never changes", which is correct for
    /// providers that own no registry: their failures are permanent until the
    /// caller's own configuration changes.
    ///
    /// [`open`]: OutputProvider::open
    fn hardware_generation(&self) -> u64 {
        0
    }
}

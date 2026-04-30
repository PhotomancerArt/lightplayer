use crate::display_pipeline::DisplayPipelineOptions;
use crate::error::OutputError;

/// Options for output driver (DisplayPipeline). Alias for DisplayPipelineOptions.
pub type OutputDriverOptions = DisplayPipelineOptions;

/// Handle for an opened output channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputChannelHandle(i32);

impl OutputChannelHandle {
    /// Create a new output channel handle
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
    /// Open an output channel
    ///
    /// # Arguments
    /// * `pin` - GPIO pin number
    /// * `byte_count` - Total number of bytes to allocate for this channel
    /// * `format` - Output format/protocol
    ///
    /// # Returns
    /// Returns `OutputChannelHandle` on success, or `OutputError` if:
    /// - Pin is already open
    /// - Invalid parameters
    /// - Hardware initialization failed
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError>;

    /// Write 16-bit RGB data to an output channel
    ///
    /// # Arguments
    /// * `handle` - Output channel handle from `open()`
    /// * `data` - 16-bit RGB data: [r,g,b; num_leds], length = num_leds * 3
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or `OutputError` if:
    /// - Handle is invalid
    /// - Data length doesn't match expected (num_leds * 3)
    /// - Hardware write failed
    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError>;

    /// Close an output channel
    ///
    /// # Arguments
    /// * `handle` - Output channel handle from `open()`
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or `OutputError` if handle is invalid
    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError>;
}

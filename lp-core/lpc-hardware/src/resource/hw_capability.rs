use serde::{Deserialize, Serialize};

/// Capability advertised by a [`crate::HwResource`].
///
/// Drivers check capabilities before claiming resources. A single resource may
/// expose multiple capabilities, such as a GPIO that can be used for input or
/// output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HwCapability {
    /// GPIO can drive an output level or waveform.
    GpioOutput,
    /// GPIO can be sampled as an input.
    GpioInput,
    /// Timing resource can drive WS281x-class LED protocols.
    Ws281xOutput,
    /// ESP-style remote-control timing peripheral.
    Rmt,
    /// Packet radio peripheral.
    Radio,
}

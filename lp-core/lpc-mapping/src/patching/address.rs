//! A place on the hardware: output, port, channel (names.html "Address").
//! Patching gives each segment the address it starts at.

use crate::patching::output_name::OutputName;
use crate::patching::port_key::PortKey;

/// Output, port, channel: "output main, port 3, channel 0".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    pub output: OutputName,
    pub port: PortKey,
    /// Counted from the port's first channel.
    pub channel: u32,
}

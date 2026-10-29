//! The cursor: the next address, as the patcher walks (names.html
//! "Cursor").

use crate::patching::output_name::OutputName;
use crate::patching::port_key::PortKey;

/// Where the next segment goes. Output and port are `None` until something
/// sets them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cursor {
    pub output: Option<OutputName>,
    pub port: Option<PortKey>,
    /// The channel, counted from the port's first.
    pub channel: u32,
    /// Lamps still to add to `channel`, written in lamps and not yet
    /// converted: they are counted in the channels per lamp of the next
    /// segment laid (values keep their unit until the patcher reaches the
    /// segment they apply to). Zero once a segment is laid.
    pub lamps: u32,
}

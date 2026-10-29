//! A start: how a node moves the cursor when the patcher enters it, one op
//! per field (names.html "Start"): for each of output, port and channel,
//! **Keep**, **Set** to a value, or **Add** a number.
//!
//! Starts are stored as ops, not as named rules: "At" and "Next port" are
//! presets that write ops (Next port = port Add 1, channel Set 0 lamps).

use crate::patching::amount::Amount;
use crate::patching::output_name::OutputName;
use crate::patching::port_key::PortKey;

/// One op per cursor field. All Keep is no start at all.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Start {
    pub output: OutputOp,
    pub port: PortOp,
    pub channel: ChannelOp,
}

/// What entering a node does to the cursor's output.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum OutputOp {
    #[default]
    Keep,
    Set(OutputName),
}

/// What entering a node does to the cursor's port.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum PortOp {
    #[default]
    Keep,
    /// The port `plus` keys after `key`, in its output's order (`plus` 0:
    /// `key` itself). Only a flow renders a `plus`: "one per port from port
    /// 4" sets child k to `{ key: 4, plus: k }`, absolute, so moving one
    /// child elsewhere leaves the others' ports alone. An authored Set has
    /// `plus` 0.
    Set { key: PortKey, plus: u32 },
    /// `n` keys after the cursor's port.
    Add(u32),
}

/// What entering a node does to the cursor's channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelOp {
    #[default]
    Keep,
    Set(Amount),
    Add(Amount),
}

impl Start {
    /// Leave the cursor where it is.
    pub const KEEP: Start = Start {
        output: OutputOp::Keep,
        port: PortOp::Keep,
        channel: ChannelOp::Keep,
    };

    /// Start at this channel of this port of this output.
    #[must_use]
    pub fn at(output: &str, port: u32, channel: Amount) -> Self {
        Start {
            output: OutputOp::Set(OutputName::new(output)),
            port: PortOp::set(port),
            channel: ChannelOp::Set(channel),
        }
    }

    /// The next port, from its first lamp: port Add 1, channel Set 0 lamps.
    #[must_use]
    pub fn next_port() -> Self {
        Start {
            output: OutputOp::Keep,
            port: PortOp::Add(1),
            channel: ChannelOp::Set(Amount::lamps(0)),
        }
    }

    #[must_use]
    pub fn is_keep(&self) -> bool {
        *self == Start::KEEP
    }
}

impl PortOp {
    /// Exactly this port.
    #[must_use]
    pub fn set(key: u32) -> Self {
        PortOp::Set {
            key: PortKey(key),
            plus: 0,
        }
    }
}

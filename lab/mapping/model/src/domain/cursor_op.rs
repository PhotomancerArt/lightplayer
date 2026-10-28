//! The walker's cursor and what moves it (theory "Patching").
//!
//! The walker (`walker.rs`) holds a cursor — an output, a port on it, and a
//! channel on that port — and every object it enters may move it, field by
//! field, by **Keep**, **Set** or **Add**. Rules on components
//! (`patch_rule.rs`) and overrides on objects (`object_override.rs`) are
//! only ways of choosing these steps; the walker sees nothing else.
//!
//! A channel is one number a port sends (DMX's word); a pixel is one
//! lamp's worth of them. Values keep the unit they were written in and are
//! converted when the walker reaches them, so "pixel 4" means 12 channels
//! on an RGB fixture and something else on an RGBW one.

use crate::domain::node::NodeId;

/// The unit an amount was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    /// Pixels, in the lamp type of the object it is on.
    Px,
    /// Channels.
    Ch,
}

/// A number of channels or pixels, as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Amount {
    pub n: u32,
    pub unit: Unit,
}

impl Amount {
    pub const ZERO: Amount = Amount::px(0);

    pub const fn px(n: u32) -> Self {
        Self { n, unit: Unit::Px }
    }

    pub const fn ch(n: u32) -> Self {
        Self { n, unit: Unit::Ch }
    }

    /// In channels, for lamps of `per_lamp` channels.
    pub fn channels(self, per_lamp: u32) -> u32 {
        match self.unit {
            Unit::Px => self.n.saturating_mul(per_lamp),
            Unit::Ch => self.n,
        }
    }

    /// Shown in pixels when it divides evenly, else in channels: `pixel 4`,
    /// `channel 13`.
    pub fn words(self, per_lamp: u32) -> String {
        let ch = self.channels(per_lamp);
        if per_lamp > 0 && ch % per_lamp == 0 {
            format!("pixel {}", ch / per_lamp)
        } else {
            format!("channel {ch}")
        }
    }
}

/// What entering an object does to the cursor's output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OutputOp {
    Keep,
    Set(NodeId),
}

/// What entering an object does to the cursor's port. Ports are keyed;
/// "the next port" is the next key in the output's order (keys have gaps
/// after deletions).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PortOp {
    Keep,
    /// The port `plus` keys after `key` (`plus` 0: `key` itself). Absolute:
    /// "one per port from D4" makes child k `Set { key: D4, plus: k }`, so
    /// moving one child elsewhere leaves the others' ports alone.
    Set {
        key: String,
        plus: u32,
    },
    /// `n` keys after the cursor's port: relative.
    Add(u32),
}

impl PortOp {
    /// Exactly this port.
    pub fn set(key: impl Into<String>) -> Self {
        PortOp::Set {
            key: key.into(),
            plus: 0,
        }
    }
}

/// What entering an object does to the cursor's channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelOp {
    Keep,
    Set(Amount),
    Add(Amount),
}

/// One step for each field of the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CursorOps {
    pub output: OutputOp,
    pub port: PortOp,
    pub channel: ChannelOp,
}

impl CursorOps {
    /// Leave the cursor where it is: "end to end".
    pub const KEEP: CursorOps = CursorOps {
        output: OutputOp::Keep,
        port: PortOp::Keep,
        channel: ChannelOp::Keep,
    };

    /// Start at this channel of this port of this output.
    pub fn at(output: &NodeId, port: impl Into<String>, channel: Amount) -> Self {
        CursorOps {
            output: OutputOp::Set(output.clone()),
            port: PortOp::set(port),
            channel: ChannelOp::Set(channel),
        }
    }

    pub fn is_keep(&self) -> bool {
        *self == CursorOps::KEEP
    }
}

/// Where the walker is: an output, a port on it, a channel on that port.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cursor {
    pub output: Option<NodeId>,
    pub port: Option<String>,
    pub channel: u32,
}

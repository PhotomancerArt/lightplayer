//! Overrides: what a person sets on one object, by its id, over the rule
//! its component renders for it (theory "Patching": "authored rules are
//! overlays over the rule the component renders for that object").
//!
//! Each cursor field is overridden on its own: give ring 3 a port and a
//! channel and its output still comes from its circle. An override never
//! moves anything but its own object's start; what follows carries on
//! from there, as the cursor does (no snapping, Yona's D1b).

use crate::domain::cursor_op::{ChannelOp, OutputOp, PortOp};

/// Which way the data runs along an object's lamps, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Direction {
    /// From its first lamp, as drawn.
    #[default]
    AsDrawn,
    /// From its last lamp.
    Reversed,
}

impl Direction {
    pub fn flipped(self) -> Direction {
        match self {
            Direction::AsDrawn => Direction::Reversed,
            Direction::Reversed => Direction::AsDrawn,
        }
    }

    pub fn is_reversed(self) -> bool {
        self == Direction::Reversed
    }
}

/// What one object overrides. Each `None` takes its rule's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct ObjectOverride {
    pub output: Option<OutputOp>,
    pub port: Option<PortOp>,
    pub channel: Option<ChannelOp>,
    /// On no pixel: it makes no span, and the cursor doesn't move past it
    /// (Yona, Q2: "having a none/null is always a good posture"). Under a
    /// container, every object below it is off too.
    pub off: bool,
    /// Which way its data runs; on a container, every object below it
    /// that doesn't say otherwise runs that way.
    pub direction: Option<Direction>,
    /// How many lamps its data entry is shifted round its own span (a
    /// dome panel entered mid-loop): wrapping, usually off.
    pub offset: Option<u32>,
}

impl ObjectOverride {
    /// Overrides nothing, so it can go.
    pub fn is_empty(&self) -> bool {
        *self == ObjectOverride::default()
    }

    /// Whether it moves where the object starts (any cursor field).
    pub fn moves_start(&self) -> bool {
        self.output.is_some() || self.port.is_some() || self.channel.is_some()
    }
}

//! One field of an override, with its value, or just which field.

use crate::patching::{ChannelOp, Direction, OutputOp, Override, PortOp};

/// One field of an [`Override`], set to a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideField {
    Output(OutputOp),
    Port(PortOp),
    Channel(ChannelOp),
    /// On no lamp (`true`), or on (`false`, the same as clearing it).
    Off(bool),
    Direction(Direction),
    /// A wrapping shift of its lamps, in lamps.
    Offset(u32),
}

/// Which field of an [`Override`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverrideFieldKind {
    Output,
    Port,
    Channel,
    Off,
    Direction,
    Offset,
}

impl OverrideField {
    /// Which field it sets.
    #[must_use]
    pub fn kind(&self) -> OverrideFieldKind {
        match self {
            Self::Output(_) => OverrideFieldKind::Output,
            Self::Port(_) => OverrideFieldKind::Port,
            Self::Channel(_) => OverrideFieldKind::Channel,
            Self::Off(_) => OverrideFieldKind::Off,
            Self::Direction(_) => OverrideFieldKind::Direction,
            Self::Offset(_) => OverrideFieldKind::Offset,
        }
    }

    /// Write it into an override.
    pub(crate) fn apply(self, entry: &mut Override) {
        match self {
            Self::Output(op) => entry.output = Some(op),
            Self::Port(op) => entry.port = Some(op),
            Self::Channel(op) => entry.channel = Some(op),
            Self::Off(off) => entry.off = off,
            Self::Direction(direction) => entry.direction = Some(direction),
            Self::Offset(offset) => entry.offset = Some(offset),
        }
    }
}

impl OverrideFieldKind {
    /// Clear it in an override: the node takes what it would get.
    pub(crate) fn clear(self, entry: &mut Override) {
        match self {
            Self::Output => entry.output = None,
            Self::Port => entry.port = None,
            Self::Channel => entry.channel = None,
            Self::Off => entry.off = false,
            Self::Direction => entry.direction = None,
            Self::Offset => entry.offset = None,
        }
    }
}

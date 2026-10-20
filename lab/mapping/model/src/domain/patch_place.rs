//! Where a fixture, a component or an object goes on the wire: one
//! placement vocabulary for all three, inherited down the tree (plan D8,
//! D9).
//!
//! A placement is three settings, each inherited on its own:
//!
//! - **Wire**: auto onto an output, a port and pixel, or not on the wire.
//! - **Direction**: as drawn, or reversed.
//! - **Turn**: how far an object's run is turned inside its own window
//!   (objects only, plan D15).
//!
//! Each one left unset **follows** the thing above it: an object its
//! component, a component the group it sits in, then the fixture. So a
//! circle placed on D0 from pixel 40 carries its rings there, end to end in
//! tree order (plan D10), and one ring can still be reversed, or put
//! somewhere else, without touching the rest. A fixture's own wire left
//! unset means its lamps go nowhere yet.
//!
//! What the placements make — which lamp lands on which pixel — is derived
//! (`patch.rs`), never stored (D7).

use crate::domain::component::ComponentId;
use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;

/// What a placement is on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatchTarget {
    /// The whole fixture: the default for everything in it.
    Fixture,
    /// A component (a circle, a repeat, a group): the default for every
    /// object under it.
    Component(ComponentId),
    /// One object (a path, a ring, a copy).
    Object(ObjectId),
}

/// Where the lamps go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wire {
    /// Flow on after every placed run on this output, in fixture order
    /// (the product's auto flow).
    Auto { output: NodeId },
    /// Start on this pixel of this port (by the port's key), and carry on
    /// end to end from there.
    At {
        output: NodeId,
        port: String,
        pixel: u32,
    },
    /// On no pixel: dark.
    Off,
}

impl Wire {
    /// The output it names, if any.
    pub fn output(&self) -> Option<&NodeId> {
        match self {
            Wire::Auto { output } | Wire::At { output, .. } => Some(output),
            Wire::Off => None,
        }
    }
}

/// Which way the data runs along what is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// From each object's first lamp, in tree order.
    #[default]
    AsDrawn,
    /// The other way: a placed component's objects in the other order,
    /// each from its last lamp.
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

/// The settings on one thing. Each `None` follows its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchPlace {
    pub on: PatchTarget,
    pub wire: Option<Wire>,
    pub direction: Option<Direction>,
    /// Objects only (plan D15): ignored anywhere else.
    pub turn: Option<u32>,
}

impl PatchPlace {
    /// A placement that sets nothing yet.
    pub fn on(target: PatchTarget) -> Self {
        Self {
            on: target,
            wire: None,
            direction: None,
            turn: None,
        }
    }

    /// Sets nothing at all, so it can go.
    pub fn is_empty(&self) -> bool {
        self.wire.is_none() && self.direction.is_none() && self.turn.is_none()
    }
}

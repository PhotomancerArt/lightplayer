//! Which way an arrow leaves its start or arrives at its end.
//!
//! The lab's edges leave a lamp heading toward the output and arrive at a
//! pixel from its left, so the head points onto the pixel. Those are two
//! different kinds of answer, and both are here: "toward the other end",
//! "on this side of the point", or an explicit direction.

use crate::{Side, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EndDirection {
    /// Straight toward the arrow's other end.
    Direct,
    /// On this side of the point: leaving from `Right` heads +x; arriving
    /// from `Left` comes in heading +x, so the head points right.
    Side(Side),
    /// This direction of travel (normalized; its length is ignored):
    /// outward at the start, inward at the end.
    Travel(Vec2),
}

impl EndDirection {
    /// The unit direction of travel at the start, for an arrow from `from`
    /// to `to`.
    pub fn leaving(self, from: Vec2, to: Vec2) -> Vec2 {
        match self {
            EndDirection::Direct => (to - from).normalized(),
            EndDirection::Side(s) => s.outward(),
            EndDirection::Travel(v) => v.normalized(),
        }
    }

    /// The unit direction of travel at the end, for an arrow from `from`
    /// to `to`.
    pub fn arriving(self, from: Vec2, to: Vec2) -> Vec2 {
        match self {
            EndDirection::Direct => (to - from).normalized(),
            EndDirection::Side(s) => -s.outward(),
            EndDirection::Travel(v) => v.normalized(),
        }
    }
}

//! Which way the data runs along a segment's lamps (names.html
//! "Direction"). Different from a shape's own lamp order ("drawn from"),
//! which is layout.

use serde::{Deserialize, Serialize};

/// Forward from lamp 0, or reverse from the last lamp. Written `"forward"`
/// or `"reverse"` in map2d format 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// From its first lamp, as drawn.
    #[default]
    Forward,
    /// From its last lamp.
    Reverse,
}

impl Direction {
    #[must_use]
    pub fn is_reverse(self) -> bool {
        self == Direction::Reverse
    }
}

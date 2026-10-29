//! A side of a point: where a line attaches to it.

use crate::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The unit vector pointing out of this side (y down: `Top` is −y).
    pub fn outward(self) -> Vec2 {
        match self {
            Side::Left => Vec2::new(-1.0, 0.0),
            Side::Right => Vec2::new(1.0, 0.0),
            Side::Top => Vec2::new(0.0, -1.0),
            Side::Bottom => Vec2::new(0.0, 1.0),
        }
    }
}

//! How a circle renders its segments: one per ring, or one for all its
//! rings (G1, 2026-10-27: one circle part, never a circle beside a ring
//! set). Named like a path's [`crate::part::PathSplit`]: the part decides
//! how it splits into segments.

use serde::{Deserialize, Serialize};

/// One segment per ring, or one for the whole circle. Written `"rings"`
/// (left out: the default) or `"one"` in map2d format 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircleSplit {
    /// Each ring its own segment, keyed by the ring's key (`circle1/r3`).
    #[default]
    Rings,
    /// Every ring on one run: one segment, at the circle's own id. What
    /// today's `ring` shape (a ring set) is.
    One,
}

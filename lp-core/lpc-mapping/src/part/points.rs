//! Points: lamps at listed positions, in order, one segment (G1,
//! 2026-10-27: "common enough"). For lamps placed by hand, where no line,
//! ring or grid spreads them (today's `PathPoints` fixtures, M3).

use alloc::vec::Vec;

/// Lamps at these positions, lamp 0 first.
#[derive(Debug, Clone, PartialEq)]
pub struct Points {
    pub at: Vec<[f32; 2]>,
}

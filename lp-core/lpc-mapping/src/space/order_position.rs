//! A lamp's position along the fixture's order (theory D27).

use crate::space::space_transform::Q16_ONE;

/// Lamp `index` of `count` along the order, `index/(count − 1)` in Q16.16,
/// rounded to nearest: the first lamp is exactly 0 and the last exactly 1,
/// however long the strip (what WLED and FastLED effects expect). One lamp
/// (or none) is at 0; an index past the end is held at 1.
#[must_use]
pub fn order_position_q16(index: u32, count: u32) -> i32 {
    if count <= 1 {
        return 0;
    }
    let last = u64::from(count - 1);
    let index = u64::from(index).min(last);
    // round(index × 2^16 / last), half up, in integers.
    ((index * Q16_ONE as u64 * 2 + last) / (2 * last)) as i32
}

//! Hardcoded `examples/basic` ring geometry — 241 lamps in 9
//! concentric rings centered at (0.5, 0.5), diameter 1.0, InnerFirst
//! order. Generated once at startup; held in a `[(f32, f32); LAMP_COUNT]`.
//!
//! Mirrors `examples/basic/src/fixture.fixture/node.toml`. If that
//! fixture changes, this table must be regenerated.

use libm::{cosf, sinf};

pub const LAMP_COUNT: usize = 241;
pub const RING_LAMP_COUNTS: [u32; 9] = [1, 8, 12, 16, 24, 32, 40, 48, 60];
const CENTER: (f32, f32) = (0.5, 0.5);
const DIAMETER: f32 = 1.0;

/// Build the 241-lamp `(x, y)` table in InnerFirst order. Coordinates
/// are normalized to `[0, 1]²`. This is invoked once during
/// `runner::run` setup.
pub fn build_lamp_positions() -> [(f32, f32); LAMP_COUNT] {
    let mut out = [(0.0_f32, 0.0_f32); LAMP_COUNT];
    let mut idx = 0;
    let max_ring_index = (RING_LAMP_COUNTS.len() - 1) as f32;
    for (ring, &lamp_count) in RING_LAMP_COUNTS.iter().enumerate() {
        let ring_radius = if max_ring_index > 0.0 {
            (DIAMETER / 2.0) * (ring as f32 / max_ring_index)
        } else {
            0.0
        };
        for lamp in 0..lamp_count {
            let angle = (2.0 * core::f32::consts::PI * lamp as f32) / lamp_count as f32;
            let x = (CENTER.0 + ring_radius * cosf(angle)).clamp(0.0, 1.0);
            let y = (CENTER.1 + ring_radius * sinf(angle)).clamp(0.0, 1.0);
            out[idx] = (x, y);
            idx += 1;
        }
    }
    out
}

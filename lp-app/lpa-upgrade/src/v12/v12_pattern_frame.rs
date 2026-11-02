//! Format 12's pattern space, frozen (`lpc-engine`
//! `products/visual/pattern_space.rs` at `f90a0388a`): the lamp box in
//! request pixels, and the exact integer affine into pattern units.

use crate::v12::v12_coordinates::{Q16_ONE, q16_to_f32};

/// The lamp box, pitch and count of one render request, in request pixels
/// (`ScopeGeometry`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V12ScopeGeometry {
    pub min_q16: [i32; 2],
    pub max_q16: [i32; 2],
    pub pitch_px: f32,
    pub lamp_count: u32,
}

impl V12ScopeGeometry {
    /// The scope of lamps at pixel-space Q16.16 points (`pixels`, in visit
    /// order), each strand opening at an index in `strand_starts`
    /// (`ScopeGeometry::from_mapping`). `None` for no lamps.
    #[must_use]
    pub fn from_pixels(pixels: &[[i32; 2]], strand_starts: &[bool]) -> Option<Self> {
        let mut count = 0u32;
        let mut min = [i32::MAX; 2];
        let mut max = [i32::MIN; 2];
        let mut previous = [0i32; 2];
        let mut distance_sum = 0.0f32;
        let mut pairs = 0u32;
        for (point, starts_strand) in pixels.iter().zip(strand_starts) {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
            if !starts_strand {
                let dx = q16_to_f32(point[0].wrapping_sub(previous[0]));
                let dy = q16_to_f32(point[1].wrapping_sub(previous[1]));
                distance_sum += libm::sqrtf(dx * dx + dy * dy);
                pairs += 1;
            }
            previous = *point;
            count += 1;
        }
        if count == 0 {
            return None;
        }
        let pitch_px = if pairs > 0 {
            distance_sum / pairs as f32
        } else {
            let width_px = q16_to_f32(max[0].wrapping_sub(min[0]));
            let height_px = q16_to_f32(max[1].wrapping_sub(min[1]));
            let area = width_px * height_px;
            if area > 0.0 {
                libm::sqrtf(area / count as f32)
            } else if count > 1 {
                width_px.max(height_px) / (count - 1) as f32
            } else {
                0.0
            }
        };
        Some(Self {
            min_q16: min,
            max_q16: max,
            pitch_px,
            lamp_count: count,
        })
    }

    /// Every texel centre of a `width` × `height` target is a lamp
    /// (`ScopeGeometry::texel_centres`).
    #[must_use]
    pub fn texel_centres(width: u32, height: u32) -> Self {
        let half = Q16_ONE / 2;
        let far = |extent: u32| -> i32 {
            let centre = (i64::from(extent.max(1)) << 16) - i64::from(half);
            centre.clamp(0, i64::from(i32::MAX)) as i32
        };
        Self {
            min_q16: [half, half],
            max_q16: [far(width), far(height)],
            pitch_px: 1.0,
            lamp_count: width.max(1).saturating_mul(height.max(1)),
        }
    }
}

/// One axis of the pixel → pattern map (`PatternAxis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct V12PatternAxis {
    origin_q16: i32,
    mul: i64,
    shift: u32,
    negate: bool,
}

impl V12PatternAxis {
    fn new(origin_q16: i32, scale: f32, negate: bool) -> Self {
        let zero = Self {
            origin_q16,
            mul: 0,
            shift: 0,
            negate,
        };
        if !(scale > 0.0) || !scale.is_finite() {
            return zero;
        }
        let bits = scale.to_bits();
        let biased = ((bits >> 23) & 0xff) as i32;
        if biased == 0 {
            return zero;
        }
        let exponent = biased - 127;
        let shift = 30 - exponent;
        if shift > 62 {
            return zero;
        }
        let mantissa = i64::from((bits & 0x007F_FFFF) | 0x0080_0000);
        Self {
            origin_q16,
            mul: mantissa << 7,
            shift: shift.max(1) as u32,
            negate,
        }
    }

    fn apply(&self, pixel_q16: i32) -> i32 {
        if self.mul == 0 {
            return 0;
        }
        let delta = (i64::from(pixel_q16) - i64::from(self.origin_q16))
            .clamp(-i64::from(i32::MAX), i64::from(i32::MAX));
        let rounded = (delta * self.mul + (1i64 << (self.shift - 1))) >> self.shift;
        let signed = if self.negate { -rounded } else { rounded };
        signed.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

/// The pixel → pattern map for one request and its three scope intrinsics
/// (`PatternFrame`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V12PatternFrame {
    x: V12PatternAxis,
    y: Option<V12PatternAxis>,
    pub extent: [f32; 2],
    pub pitch: f32,
    pub lamp_count: u32,
}

impl V12PatternFrame {
    /// A 2D frame: the lamp box centred, long side −1…1, y up
    /// (`PatternFrame::two_d`).
    #[must_use]
    pub fn two_d(scope: &V12ScopeGeometry) -> Self {
        let span = |axis: usize| {
            q16_to_f32(scope.max_q16[axis].wrapping_sub(scope.min_q16[axis])).max(0.0) / 2.0
        };
        let centre = |axis: usize| {
            ((i64::from(scope.min_q16[axis]) + i64::from(scope.max_q16[axis])) >> 1) as i32
        };
        let half = [span(0), span(1)];
        let half_long = half[0].max(half[1]);
        let (scale, extent, pitch) = if half_long > 0.0 {
            let scale = 1.0 / half_long;
            let extent = if half[0] >= half[1] {
                [1.0, half[1] * scale]
            } else {
                [half[0] * scale, 1.0]
            };
            (scale, extent, scope.pitch_px * scale)
        } else {
            (0.0, [0.0, 0.0], 2.0)
        };
        Self {
            x: V12PatternAxis::new(centre(0), scale, false),
            y: Some(V12PatternAxis::new(centre(1), scale, true)),
            extent,
            pitch,
            lamp_count: scope.lamp_count,
        }
    }

    /// A 1D frame for a native strip request (`PatternFrame::one_d_strip`).
    #[must_use]
    pub fn one_d_strip(count: u32) -> Self {
        let step = if count > 1 {
            1.0 / (count - 1) as f32
        } else {
            0.0
        };
        Self {
            x: V12PatternAxis::new(Q16_ONE / 2, step, false),
            y: None,
            extent: [0.5, 0.0],
            pitch: if count > 1 { step } else { 1.0 },
            lamp_count: count,
        }
    }

    /// A 1D frame for a 1D shader answering a 2D request
    /// (`PatternFrame::one_d_projected`).
    #[must_use]
    pub fn one_d_projected(width: u32, lamp_count: u32) -> Self {
        let scale = if width > 0 { 1.0 / width as f32 } else { 0.0 };
        Self {
            x: V12PatternAxis::new(0, scale, false),
            y: None,
            extent: [0.5, 0.0],
            pitch: if lamp_count > 1 {
                1.0 / (lamp_count - 1) as f32
            } else {
                1.0
            },
            lamp_count,
        }
    }

    /// Map packed words in place: `[x, y]` pairs in 2D, single words in 1D
    /// (`PatternFrame::map_points_in_place`).
    pub fn map_words(&self, words: &mut [i32]) {
        match self.y {
            Some(y) => {
                for pair in words.chunks_exact_mut(2) {
                    pair[0] = self.x.apply(pair[0]);
                    pair[1] = y.apply(pair[1]);
                }
            }
            None => {
                for word in words.iter_mut() {
                    *word = self.x.apply(*word);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip runs 0 → 1, first lamp to last.
    #[test]
    fn a_strip_runs_zero_to_one() {
        let frame = V12PatternFrame::one_d_strip(73);
        let mut words = [Q16_ONE / 2, 72 * Q16_ONE + Q16_ONE / 2];
        frame.map_words(&mut words);
        assert_eq!(words, [0, Q16_ONE]);
    }

    /// Texel centres of a square target map to ±1, y up.
    #[test]
    fn texel_centres_of_a_square_map_to_plus_minus_one() {
        let frame = V12PatternFrame::two_d(&V12ScopeGeometry::texel_centres(16, 16));
        let mut words = [Q16_ONE / 2, Q16_ONE / 2];
        frame.map_words(&mut words);
        assert_eq!(words, [-Q16_ONE, Q16_ONE]);
        assert_eq!(frame.extent, [1.0, 1.0]);
    }
}

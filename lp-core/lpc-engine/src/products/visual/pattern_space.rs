//! Pattern space: the coordinate frame an opted-in shader's `pos` arrives in.
//!
//! > **The shader sees the lamps, not a texture.** Pattern space is the
//! > lamps' bounding box, centred at the origin, scaled uniformly so its long
//! > side runs −1…1, with y up. In 1D, `pos` runs 0 → 1 from the strand's
//! > first lamp to its last.
//!
//! (`docs/adr/2026-09-24-pattern-space.md`.) A shader opts in with
//! `"coords": "pattern"` ([`lpc_model::ShaderCoords`]); every other shader
//! keeps the pixel frame, untouched.
//!
//! # Where the transform happens
//!
//! Host-side, on the Q16.16 sample coordinates, just before they reach the
//! backend. Every backend — `lpvm-native` rv32 and Xtensa, `lpvm-wasm`, the
//! GPU tier — consumes the same coordinate words, so one transform here is
//! the rule on all of them, and nothing about it is compiled into a program
//! (flipping the opt-in costs no recompile). It is an exact integer affine
//! per axis ([`PatternAxis`]): an in-shader `pos * scale + offset` in Q16.16
//! would quantize `scale` to 1/65536, which is a visible zoom error on a long
//! strip (1/(N−1) for N = 30 000 is two ulps).
//!
//! # Scope geometry
//!
//! [`ScopeGeometry`] is what a consumer lends the producer about the surface
//! being rendered: the lamp box in request pixels, the pitch, the lamp count.
//! A fixture computes it once per mapping version, in one O(n), O(1)-memory
//! pass over exactly the coordinates its sampler streams. A request with no
//! lamps behind it (a Studio canvas, a preview) gets
//! [`ScopeGeometry::texel_centres`]: every texel centre is a lamp.

use lpc_model::nodes::fixture::{MappingRef, for_each_mapping_center_in_strands};

use super::coordinates::{Q16_ONE, normalized_f32_to_q16, normalized_q16_to_pixel_q16};

/// The whole-surface ("scope") geometry of one render request, in the
/// request's own pixel frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopeGeometry {
    /// Lamp box minimum corner, pixel-space Q16.16 (y down, as the request).
    pub min_q16: [i32; 2],
    /// Lamp box maximum corner, pixel-space Q16.16.
    pub max_q16: [i32; 2],
    /// Typical lamp-to-lamp spacing in request pixels: the mean distance
    /// between consecutive lamps within strands (pre-ruling DD1), with the
    /// fallbacks [`Self::from_mapping`] documents.
    pub pitch_px: f32,
    /// Number of lamps.
    pub lamp_count: u32,
}

impl ScopeGeometry {
    /// The scope geometry of a fixture mapping rendered at
    /// `width` × `height`, or `None` for a mapping with no lamps.
    ///
    /// One pass, O(n) time, O(1) memory. The box is taken over the very
    /// Q16.16 pixel coordinates the Direct fill streams (same conversion, same
    /// clamp), so the extreme lamps land on exactly ±1.
    ///
    /// Pitch (pre-ruling DD1): the mean distance between consecutive lamps
    /// within a strand — a strand with one lamp contributes nothing. With no
    /// pairs at all it falls back to `sqrt(box area / count)`, and when that
    /// area is zero too (a row of one-lamp strands) to the long side over
    /// `count − 1`. A single lamp has pitch 0 here; [`PatternFrame`] gives
    /// that degenerate scope its own answer.
    pub fn from_mapping(mapping: MappingRef<'_>, width: u32, height: u32) -> Option<Self> {
        let mut count = 0u32;
        let mut min = [i32::MAX; 2];
        let mut max = [i32::MIN; 2];
        let mut previous = [0i32; 2];
        let mut distance_sum = 0.0f32;
        let mut pairs = 0u32;
        for_each_mapping_center_in_strands(mapping, |starts_strand, [x, y]| {
            let point = [
                normalized_q16_to_pixel_q16(normalized_f32_to_q16(x), width),
                normalized_q16_to_pixel_q16(normalized_f32_to_q16(y), height),
            ];
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
            previous = point;
            count += 1;
        });
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

    /// The scope of a request with no lamps behind it: every texel centre of
    /// a `width` × `height` target is a lamp, one pixel apart.
    #[must_use]
    pub fn texel_centres(width: u32, height: u32) -> Self {
        let half = Q16_ONE / 2;
        let far = |extent: u32| -> i32 {
            // Saturating: an extent past 32 767 px has no Q16.16 pixel frame.
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

/// One axis of the pixel → pattern map, in exact integer arithmetic:
/// `pattern = (pixel − origin) × scale`, both sides Q16.16, with `scale`
/// carried as a 31-bit mantissa and a shift so it keeps its full f32
/// precision (see the module docs for why it is not a Q16.16 constant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatternAxis {
    origin_q16: i32,
    /// `scale × 2^shift`, in `[2^30, 2^31)`; `0` maps every point to 0.
    mul: i64,
    shift: u32,
    /// Negate the result — the y axis, so pattern space is y up.
    negate: bool,
}

impl PatternAxis {
    /// `pattern = (pixel − origin) × scale`, negated when `negate`.
    #[must_use]
    pub fn new(origin_q16: i32, scale: f32, negate: bool) -> Self {
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
            // Subnormal: far below anything a pixel frame produces.
            return zero;
        }
        // scale = m × 2^e with m ∈ [1, 2): mul = m × 2^30, so
        // scale = mul × 2^(e − 30) and the shift is 30 − e.
        let exponent = biased - 127;
        let shift = 30 - exponent;
        if shift > 62 {
            return zero;
        }
        let mantissa = i64::from((bits & 0x007F_FFFF) | 0x0080_0000);
        // A scale ≥ 2^30 per pixel would need a shift below 1: no lamp box is
        // that small (the smallest non-zero one is an ulp, scale 2^16). Cap
        // it rather than overflow.
        Self {
            origin_q16,
            mul: mantissa << 7,
            shift: shift.max(1) as u32,
            negate,
        }
    }

    /// Map one Q16.16 pixel coordinate to Q16.16 pattern units, rounded to
    /// nearest and saturated to `i32`.
    #[must_use]
    pub fn apply(&self, pixel_q16: i32) -> i32 {
        if self.mul == 0 {
            return 0;
        }
        // |delta| < 2^31 after the clamp and mul < 2^31, so the product and
        // the rounding term stay inside i64.
        let delta = (i64::from(pixel_q16) - i64::from(self.origin_q16))
            .clamp(-i64::from(i32::MAX), i64::from(i32::MAX));
        let rounded = (delta * self.mul + (1i64 << (self.shift - 1))) >> self.shift;
        let signed = if self.negate { -rounded } else { rounded };
        signed.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

/// The pixel → pattern map for one request, plus the three scope intrinsics
/// the shader reads alongside it (`patternExtent`, `patternPitch`,
/// `lampCount`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternFrame {
    x: PatternAxis,
    /// The y axis of a 2D frame; `None` for a 1D (single-word) frame.
    y: Option<PatternAxis>,
    /// Half-size of the lamp box in pattern units.
    pub extent: [f32; 2],
    /// Mean consecutive-lamp distance in pattern units.
    pub pitch: f32,
    /// Number of lamps.
    pub lamp_count: u32,
}

impl PatternFrame {
    /// A 2D frame for `[x, y]` pixel pairs: the lamp box centred at the
    /// origin, long side −1…1, y up.
    ///
    /// A zero-size box (one lamp, or every lamp on one spot) maps every
    /// point to the origin with extent `(0, 0)` and pitch 2 — one lamp
    /// spanning the whole piece — so a pattern that divides by pitch never
    /// divides by zero.
    #[must_use]
    pub fn two_d(scope: &ScopeGeometry) -> Self {
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
            // The long side is exactly 1 by construction; say so exactly.
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
            x: PatternAxis::new(centre(0), scale, false),
            y: Some(PatternAxis::new(centre(1), scale, true)),
            extent,
            pitch,
            lamp_count: scope.lamp_count,
        }
    }

    /// A 1D frame for a native strip request: `count` lamps at texel centres
    /// `k + 0.5` of an `(count, 1)` target, lamp 0 → 0 and lamp `count − 1`
    /// → 1. Extent `(0.5, 0)` (the strip spans 0…1), pitch `1 / (count − 1)`.
    /// One lamp sits at 0 with pitch 1.
    #[must_use]
    pub fn one_d_strip(count: u32) -> Self {
        let step = if count > 1 {
            1.0 / (count - 1) as f32
        } else {
            0.0
        };
        Self {
            x: PatternAxis::new(Q16_ONE / 2, step, false),
            y: None,
            extent: [0.5, 0.0],
            pitch: if count > 1 { step } else { 1.0 },
            lamp_count: count,
        }
    }

    /// A 1D frame for a 1D shader answering a 2D request through a
    /// projection cell: the projected strip coordinate arrives as
    /// `t × width` pixels and pattern `pos` is `t` itself. The lamp count is
    /// the 2D request's; the pitch assumes they spread evenly along the strip.
    #[must_use]
    pub fn one_d_projected(width: u32, lamp_count: u32) -> Self {
        let scale = if width > 0 { 1.0 / width as f32 } else { 0.0 };
        Self {
            x: PatternAxis::new(0, scale, false),
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

    /// Map the first `n` points of `coords` (packed for this frame: `[x, y]`
    /// pairs in 2D, single words in 1D) into `out`, zeroing the tail of `out`
    /// past them — the packing a fresh upload would have.
    pub fn map_points(&self, coords: &[i32], n: usize, out: &mut [i32]) {
        match self.y {
            Some(y) => {
                for (src, dst) in coords[..n * 2].chunks_exact(2).zip(out.chunks_exact_mut(2)) {
                    dst[0] = self.x.apply(src[0]);
                    dst[1] = y.apply(src[1]);
                }
                out[n * 2..].fill(0);
            }
            None => {
                for (src, dst) in coords[..n].iter().zip(out.iter_mut()) {
                    *dst = self.x.apply(*src);
                }
                out[n..].fill(0);
            }
        }
    }

    /// [`Self::map_points`] over a buffer this node owns.
    pub fn map_points_in_place(&self, coords: &mut [i32], n: usize) {
        match self.y {
            Some(y) => {
                for pair in coords[..n * 2].chunks_exact_mut(2) {
                    pair[0] = self.x.apply(pair[0]);
                    pair[1] = y.apply(pair[1]);
                }
            }
            None => {
                for word in &mut coords[..n] {
                    *word = self.x.apply(*word);
                }
            }
        }
    }

    /// Map one pixel-space point; test- and tooling-facing (`[x, y]`, `y`
    /// ignored in 1D).
    #[must_use]
    pub fn map_point(&self, pixel_q16: [i32; 2]) -> [i32; 2] {
        [
            self.x.apply(pixel_q16[0]),
            self.y.map_or(0, |y| y.apply(pixel_q16[1])),
        ]
    }
}

fn q16_to_f32(value: i32) -> f32 {
    value as f32 / Q16_ONE as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q16(value: f32) -> i32 {
        (value * Q16_ONE as f32) as i32
    }

    fn unq16(value: i32) -> f32 {
        value as f32 / Q16_ONE as f32
    }

    /// Two Q16.16 ulps: the box corners land on ±1 to within rounding.
    const EPS: f32 = 2.0 / 65536.0;

    #[test]
    fn a_square_box_maps_its_corners_to_plus_minus_one_y_up() {
        let frame = PatternFrame::two_d(&ScopeGeometry::texel_centres(16, 16));
        let top_left = frame.map_point([q16(0.5), q16(0.5)]);
        let bottom_right = frame.map_point([q16(15.5), q16(15.5)]);
        assert!(libm::fabsf(unq16(top_left[0]) + 1.0) <= EPS, "{top_left:?}");
        assert!(
            libm::fabsf(unq16(top_left[1]) - 1.0) <= EPS,
            "y up: {top_left:?}"
        );
        assert!(libm::fabsf(unq16(bottom_right[0]) - 1.0) <= EPS);
        assert!(libm::fabsf(unq16(bottom_right[1]) + 1.0) <= EPS);
        assert_eq!(frame.extent, [1.0, 1.0]);
        assert!(
            libm::fabsf(frame.pitch - 2.0 / 15.0) < 1e-6,
            "{}",
            frame.pitch
        );
        assert_eq!(frame.lamp_count, 256);
    }

    #[test]
    fn the_long_side_sets_the_scale_and_the_short_side_is_centred() {
        let scope = ScopeGeometry {
            min_q16: [q16(10.0), q16(20.0)],
            max_q16: [q16(50.0), q16(30.0)],
            pitch_px: 4.0,
            lamp_count: 11,
        };
        let frame = PatternFrame::two_d(&scope);
        assert_eq!(frame.extent, [1.0, 0.25]);
        assert!(libm::fabsf(frame.pitch - 0.2) < 1e-6);
        let centre = frame.map_point([q16(30.0), q16(25.0)]);
        assert_eq!(centre, [0, 0]);
        let top = frame.map_point([q16(30.0), q16(20.0)]);
        assert!(libm::fabsf(unq16(top[1]) - 0.25) <= EPS, "{top:?}");
    }

    #[test]
    fn a_straight_strip_has_zero_height_and_no_nan() {
        let scope = ScopeGeometry {
            min_q16: [q16(0.5), q16(3.0)],
            max_q16: [q16(99.5), q16(3.0)],
            pitch_px: 1.0,
            lamp_count: 100,
        };
        let frame = PatternFrame::two_d(&scope);
        assert_eq!(frame.extent, [1.0, 0.0]);
        assert!(frame.pitch.is_finite() && frame.pitch > 0.0);
        let mid = frame.map_point([q16(50.0), q16(3.0)]);
        assert_eq!(mid[1], 0);
    }

    #[test]
    fn one_lamp_is_the_origin_with_a_whole_piece_pitch() {
        let scope = ScopeGeometry {
            min_q16: [q16(4.0), q16(4.0)],
            max_q16: [q16(4.0), q16(4.0)],
            pitch_px: 0.0,
            lamp_count: 1,
        };
        let frame = PatternFrame::two_d(&scope);
        assert_eq!(frame.map_point([q16(4.0), q16(4.0)]), [0, 0]);
        assert_eq!(frame.extent, [0.0, 0.0]);
        assert_eq!(frame.pitch, 2.0);
    }

    #[test]
    fn a_strip_runs_zero_to_one_first_lamp_to_last() {
        for count in [2u32, 7, 150, 30_000] {
            let frame = PatternFrame::one_d_strip(count);
            let first = frame.map_point([Q16_ONE / 2, 0])[0];
            let last = frame.map_point([((count - 1) as i32) * Q16_ONE + Q16_ONE / 2, 0])[0];
            assert_eq!(first, 0, "count {count}");
            assert!((last - Q16_ONE).abs() <= 1, "count {count}: last {last}");
        }
        let one = PatternFrame::one_d_strip(1);
        assert_eq!(one.map_point([Q16_ONE / 2, 0])[0], 0);
        assert_eq!(one.pitch, 1.0);
    }

    /// The reason the transform is an integer affine and not a Q16.16
    /// constant: a 30 000-lamp strip's step is two ulps, and a quantized
    /// constant would land the last lamp near 0.92 instead of 1.
    #[test]
    fn a_dome_scale_strip_keeps_full_precision_along_its_length() {
        let count = 30_000u32;
        let frame = PatternFrame::one_d_strip(count);
        for k in [1u32, 1000, 14_999, 29_998] {
            let got = frame.map_point([(k as i32) * Q16_ONE + Q16_ONE / 2, 0])[0];
            let want = (f64::from(k) / f64::from(count - 1) * 65536.0).round() as i32;
            assert!((got - want).abs() <= 1, "lamp {k}: {got} vs {want}");
        }
    }

    #[test]
    fn map_points_zeroes_the_tail_like_a_fresh_upload() {
        let frame = PatternFrame::two_d(&ScopeGeometry::texel_centres(4, 4));
        let coords = [q16(0.5), q16(0.5), q16(3.5), q16(3.5), 77, 77];
        let mut out = [9i32; 6];
        frame.map_points(&coords, 2, &mut out);
        assert_eq!(&out[4..], &[0, 0]);
        assert!(libm::fabsf(unq16(out[0]) + 1.0) <= EPS);
    }
}

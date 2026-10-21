//! Pattern mode's fit of the solver grid over the lamps' box.
//!
//! A pattern-mode Fluid (`"coords": "pattern"`) spans its grid over the lamp
//! box of the scope it renders for, edge to edge: grid uv `(0, 0)` is the
//! box's top-left corner, `(1, 1)` its bottom-right. Pattern space is that
//! same box centred at the origin, long side −1…1, y up
//! (`docs/adr/2026-09-24-pattern-space.md`), so the fit is one scale per
//! axis — `1 / (2 × half-extent)` grid uv per pattern unit — and a y flip.
//!
//! A box with no extent on an axis (a straight strip, a single lamp) gives
//! that axis the long side's span instead (±1 pattern unit), so the grid
//! keeps a height and a round stamp stays round; every lamp of such a box
//! sits on that axis's centre line, and so does every sample.

use crate::products::visual::coordinates::Q16_ONE;
use crate::products::visual::{PatternFrame, ScopeGeometry, VisualSpace};

/// Grid uv per pattern unit, per axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternGridFit {
    scale: [f32; 2],
    /// `scale` in Q16.16, for the per-sample path.
    scale_q16: [i64; 2],
}

impl PatternGridFit {
    /// The fit for a lamp box of half-size `extent` in pattern units (a
    /// [`PatternFrame::extent`]).
    #[must_use]
    pub fn new(extent: [f32; 2]) -> Self {
        let long = extent[0].max(extent[1]);
        let long = if long > 0.0 && long.is_finite() {
            long
        } else {
            1.0
        };
        let half = |axis: f32| {
            if axis > 0.0 && axis.is_finite() {
                axis
            } else {
                long
            }
        };
        let scale = [0.5 / half(extent[0]), 0.5 / half(extent[1])];
        let to_q16 = |value: f32| libm::roundf(value * Q16_ONE as f32) as i64;
        Self {
            scale,
            scale_q16: [to_q16(scale[0]), to_q16(scale[1])],
        }
    }

    /// The fit for a scope's lamp box.
    #[must_use]
    pub fn from_scope(scope: &ScopeGeometry) -> Self {
        Self::new(PatternFrame::two_d(scope).extent)
    }

    /// A pattern-space point (y up) → grid uv (0 → 1 edge to edge, y down).
    /// Not clamped: a point off the box lands outside 0…1.
    #[must_use]
    pub fn pattern_to_uv(&self, pattern: [f32; 2]) -> [f32; 2] {
        [
            0.5 + pattern[0] * self.scale[0],
            0.5 - pattern[1] * self.scale[1],
        ]
    }

    /// A pattern-space vector (y up) → a grid-uv vector (y down).
    #[must_use]
    pub fn vector_to_uv(&self, vector: [f32; 2]) -> [f32; 2] {
        [vector[0] * self.scale[0], -vector[1] * self.scale[1]]
    }

    /// A pattern-space radius → the ellipse's grid-uv half-axes, round on
    /// the lamps.
    #[must_use]
    pub fn radius_to_uv(&self, radius: f32) -> [f32; 2] {
        [radius * self.scale[0], radius * self.scale[1]]
    }

    /// [`Self::pattern_to_uv`] in Q16.16, for the per-sample path. Not
    /// clamped (the sampler clamps).
    #[must_use]
    pub fn pattern_q16_to_uv_q16(&self, pattern: [i32; 2]) -> [i32; 2] {
        let half = i64::from(Q16_ONE / 2);
        let u = half + ((i64::from(pattern[0]) * self.scale_q16[0]) >> 16);
        let v = half - ((i64::from(pattern[1]) * self.scale_q16[1]) >> 16);
        let clamp = |value: i64| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        [clamp(u), clamp(v)]
    }
}

/// How one pattern-mode render request places its points on the grid: the
/// frame its pixel coordinates map through, the grid's fit over that frame's
/// lamp box, and the scope it stands for.
///
/// - **2D request:** the consumer's lamp box, or — when it lent none (a
///   Studio canvas, a preview) — every texel centre of the request is a lamp
///   ([`ScopeGeometry::texel_centres`]), as for a pattern-space shader.
/// - **1D request** (an `(N, 1)` strip, from a fixture with no 2D mapping):
///   the Fluid is a 2D product, so it answers the way a 2D pattern shader
///   does — the strip is laid along the centre line, x −1…1 from the first
///   lamp to the last at y 0. Its scope is the strip's `N` texel centres;
///   the grid spans it in x and, the strip having no height, ±1 in y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternRequestFit {
    frame: PatternFrame,
    fit: PatternGridFit,
    space: VisualSpace,
    /// The scope these points stand for.
    pub scope: ScopeGeometry,
    /// Whether real lamps are behind the request (a lent scope, or a strip),
    /// rather than the texel-centre stand-in.
    pub has_lamps: bool,
}

impl PatternRequestFit {
    #[must_use]
    pub fn new(space: VisualSpace, width: u32, height: u32, scope: Option<ScopeGeometry>) -> Self {
        let (scope, has_lamps) = match (space, scope) {
            (VisualSpace::TwoD, Some(scope)) => (scope, true),
            (VisualSpace::TwoD, None) => (ScopeGeometry::texel_centres(width, height), false),
            (VisualSpace::OneD, _) => (ScopeGeometry::texel_centres(width, 1), true),
        };
        let frame = PatternFrame::two_d(&scope);
        Self {
            frame,
            fit: PatternGridFit::new(frame.extent),
            space,
            scope,
            has_lamps,
        }
    }

    /// Grid uv (Q16.16) for one point of the request, packed as the request
    /// packs it: `point[0..2]` is `[x, y]` in 2D, `point[0]` is `t` in 1D.
    #[must_use]
    pub fn uv_q16(&self, point: &[i32]) -> [i32; 2] {
        let pixel = match self.space {
            VisualSpace::TwoD => [point[0], point[1]],
            // The strip's texel-centre box has its one row at y 0.5 px.
            VisualSpace::OneD => [point[0], Q16_ONE / 2],
        };
        self.fit.pattern_q16_to_uv_q16(self.frame.map_point(pixel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q16(value: f32) -> i32 {
        (value * Q16_ONE as f32) as i32
    }

    #[test]
    fn a_square_box_spans_the_grid_with_y_up() {
        let fit = PatternGridFit::new([1.0, 1.0]);
        assert_eq!(fit.pattern_to_uv([0.0, 0.0]), [0.5, 0.5]);
        assert_eq!(fit.pattern_to_uv([-1.0, 1.0]), [0.0, 0.0], "top-left");
        assert_eq!(fit.pattern_to_uv([1.0, -1.0]), [1.0, 1.0], "bottom-right");
        assert_eq!(fit.vector_to_uv([0.0, 1.0]), [0.0, -0.5], "up is -v");
    }

    #[test]
    fn a_two_to_one_box_scales_each_axis_to_its_extent() {
        let fit = PatternGridFit::new([1.0, 0.5]);
        assert_eq!(fit.pattern_to_uv([1.0, 0.5]), [1.0, 0.0]);
        // Round on the lamps: half the grid's width is 1 pattern unit, half
        // its height 0.5, so a radius covers twice the uv vertically.
        assert_eq!(fit.radius_to_uv(0.25), [0.125, 0.25]);
    }

    #[test]
    fn a_flat_axis_takes_the_long_side_and_never_divides_by_zero() {
        let strip = PatternGridFit::new([1.0, 0.0]);
        assert_eq!(strip.radius_to_uv(0.5), [0.25, 0.25]);
        let point = PatternGridFit::new([0.0, 0.0]);
        assert_eq!(point.pattern_to_uv([0.0, 0.0]), [0.5, 0.5]);
        assert!(point.radius_to_uv(1.0).iter().all(|v| v.is_finite()));
    }

    #[test]
    fn the_q16_path_agrees_with_the_f32_path() {
        let fit = PatternGridFit::new([1.0, 0.25]);
        for p in [[0.0, 0.0], [1.0, 0.25], [-0.5, -0.125], [0.3, 0.1]] {
            let want = fit.pattern_to_uv(p);
            let got = fit.pattern_q16_to_uv_q16([q16(p[0]), q16(p[1])]);
            for axis in 0..2 {
                let got = got[axis] as f32 / Q16_ONE as f32;
                assert!(
                    (got - want[axis]).abs() < 3.0 / 65536.0,
                    "{p:?}: {got} vs {want:?}"
                );
            }
        }
    }

    /// A lent lamp box off-centre in the request: its centre is the grid
    /// centre, its corners the grid corners — the texture's margin is not.
    #[test]
    fn a_lent_box_is_the_grid_not_the_request() {
        let scope = ScopeGeometry {
            min_q16: [q16(2.0), q16(4.0)],
            max_q16: [q16(26.0), q16(16.0)],
            pitch_px: 1.0,
            lamp_count: 10,
        };
        let request = PatternRequestFit::new(VisualSpace::TwoD, 32, 32, Some(scope));
        assert!(request.has_lamps);
        assert_uv(request.uv_q16(&[q16(14.0), q16(10.0)]), [32768, 32768]);
        assert_uv(request.uv_q16(&[q16(2.0), q16(4.0)]), [0, 0]);
        assert_uv(request.uv_q16(&[q16(26.0), q16(16.0)]), [65536, 65536]);
        // The request's own centre is right of and below the box's centre.
        let centre = request.uv_q16(&[q16(16.0), q16(16.0)]);
        assert!(
            centre[0] > 32768 + 1000 && centre[1] > 32768 + 1000,
            "{centre:?}"
        );
    }

    #[test]
    fn a_strip_runs_along_the_grid_centre_line() {
        let request = PatternRequestFit::new(VisualSpace::OneD, 11, 1, None);
        assert!(request.has_lamps);
        assert_uv(request.uv_q16(&[q16(0.5)]), [0, 32768]);
        assert_uv(request.uv_q16(&[q16(5.5)]), [32768, 32768]);
        assert_uv(request.uv_q16(&[q16(10.5)]), [65536, 32768]);
    }

    /// Within the integer affine's rounding (a couple of Q16.16 ulps).
    fn assert_uv(got: [i32; 2], want: [i32; 2]) {
        for axis in 0..2 {
            assert!((got[axis] - want[axis]).abs() <= 2, "{got:?} vs {want:?}");
        }
    }
}

//! T: the one affine from a fixture's space to a source's space.

use crate::space::fit::Fit;
use crate::space::fit_mode::FitMode;
use crate::space::outside_rule::OutsideRule;
use crate::space::product_box::{ProductBox, ProductBoxKind};
use crate::space::sample::Sample;
use crate::space::space_error::SpaceError;
use crate::space::view_box::ViewBox;

/// Q16.16 one: the source coordinates T writes.
pub const Q16_ONE: i32 = 1 << 16;

/// Q2.30 one: a box-relative coordinate, `0` at the box's low edge and
/// `REL_ONE` at its high edge. A point up to one box size outside either edge
/// still has a value (Q2.30 spans −2…2); farther out it saturates, which is
/// still outside, so still dark.
pub const REL_ONE: i32 = 1 << 30;

/// T for one fixture and one source (theory D3, D5, D16): from a point in
/// the fixture's space to the coordinate the source sees there.
///
/// Built from the fixture's box, its fit and the source's box. It is six
/// numbers, so a source change (another shader, a fluid, a cross-fade's
/// second entry) builds another one and touches no lamp (D5, R8).
///
/// # The arithmetic
///
/// T is built in `f64` and kept as two rows of integer coefficients over
/// box-relative coordinates (Q2.30, [`REL_ONE`]):
///
/// ```text
/// q16 = round((M_u · u + M_v · v + C) / 2^shift)      per output axis
/// ```
///
/// Each row's coefficients share one shift, chosen so the larger of `M_u`,
/// `M_v` has 29 bits and `C` fits beside them; every product and the sum
/// stay inside `i64`, the result is rounded to nearest (half up) and
/// saturated to `i32`. That is pattern space's technique (`PatternAxis`,
/// `docs/adr/2026-09-24-pattern-space.md`), extended to a 2×2 matrix for
/// rotation: a coefficient is never a Q16.16 constant, so a 30,000-lamp
/// strip's last lamp lands on exactly 1.0, and a box in millimetres metres
/// long (fixture units are anything) never overflows, because the per-lamp
/// input is a fraction of the box, not a length.
///
/// The fit's y flip (a y-down fixture, a y-up interface) is inside the
/// coefficients and never seen (D16).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpaceTransform {
    view_box: ViewBox,
    product: ProductBox,
    /// The affine over box-relative coordinates in `f64`: `q = rel[i][0]·u
    /// + rel[i][1]·v + rel[i][2]`, `u`, `v` fractions of the box.
    rel: [[f64; 3]; 2],
    rows: [FixedRow; 2],
    /// A bounded product's extent in Q16.16, `[min, max]` per axis; `None`
    /// for a view.
    extent_q16: Option<[[i32; 2]; 2]>,
}

impl SpaceTransform {
    /// Build T for a fixture's `view_box` and `fit` against a source's
    /// `product` box. Refuses a scale that is not positive and finite and a
    /// rotation that is not finite.
    pub fn new(view_box: ViewBox, fit: Fit, product: ProductBox) -> Result<Self, SpaceError> {
        if !(fit.scale.is_finite() && fit.scale > 0.0) {
            return Err(SpaceError::BadScale);
        }
        if !fit.rotation_deg.is_finite() {
            return Err(SpaceError::BadRotation);
        }
        let rel = relative_affine(&view_box, &fit, &product);
        let rows = [FixedRow::new(rel[0]), FixedRow::new(rel[1])];
        let extent_q16 = match product.kind() {
            ProductBoxKind::View => None,
            ProductBoxKind::Extent => {
                let axis = |min: f32, size: f32| {
                    let min = f64::from(min);
                    [to_q16(min), to_q16(min + f64::from(size))]
                };
                Some([
                    axis(product.min_x(), product.width()),
                    axis(product.min_y(), product.height()),
                ])
            }
        };
        Ok(Self {
            view_box,
            product,
            rel,
            rows,
            extent_q16,
        })
    }

    /// The fixture's box this T was built for.
    #[must_use]
    pub fn view_box(&self) -> &ViewBox {
        &self.view_box
    }

    /// The source's box this T was built for.
    #[must_use]
    pub fn product(&self) -> &ProductBox {
        &self.product
    }

    /// Where a lamp at fixture-space `point` lands: [`Sample::Dark`] outside
    /// the fixture's box (edges inside), else the source coordinate.
    #[must_use]
    pub fn apply(&self, point: [f32; 2]) -> Sample {
        if !self.view_box.contains(point) {
            return Sample::Dark;
        }
        let [u, v] = self.view_box.relative(point);
        self.classify(self.map_rel([to_rel(u), to_rel(v)]))
    }

    /// Where a lamp at box-relative `(u, v)` lands (Q2.30, [`REL_ONE`] the
    /// box's far edge): the engine's per-lamp entry point, which holds lamps
    /// as fractions of the box so fixture units can be anything.
    #[must_use]
    pub fn apply_rel(&self, rel: [i32; 2]) -> Sample {
        let inside = |c: i32| (0..=REL_ONE).contains(&c);
        if !inside(rel[0]) || !inside(rel[1]) {
            return Sample::Dark;
        }
        self.classify(self.map_rel(rel))
    }

    /// What the source is evaluated at for `sample`, after the fixture's
    /// outside rule: the coordinate, or `None` when the source is not run
    /// there — a dark lamp, or a [`crate::space::AddressMode::Solid`] axis
    /// past a bounded extent (the rule's colour, D18). Callers tell the two
    /// apart by the sample: a dark lamp is black whatever the rule says.
    #[must_use]
    pub fn resolve(&self, sample: Sample, rule: &OutsideRule) -> Option<[i32; 2]> {
        match sample {
            Sample::Dark => None,
            Sample::At(q) => Some(q),
            Sample::Outside(q) => {
                let Some([x, y]) = self.extent_q16 else {
                    return Some(q);
                };
                Some([
                    rule.x.resolve(q[0], x[0], x[1])?,
                    rule.y.resolve(q[1], y[0], y[1])?,
                ])
            }
        }
    }

    /// T in the fixture's own units, in `f64`: `q = m[i][0]·x + m[i][1]·y +
    /// m[i][2]`. For a picture drawn through T (Studio) and for reports;
    /// lamps go through [`Self::apply`] or [`Self::apply_rel`].
    #[must_use]
    pub fn affine(&self) -> [[f64; 3]; 2] {
        let (w, h) = (
            f64::from(self.view_box.width()),
            f64::from(self.view_box.height()),
        );
        let (x0, y0) = (
            f64::from(self.view_box.min_x()),
            f64::from(self.view_box.min_y()),
        );
        let row = |r: [f64; 3]| {
            let (a, b) = (r[0] / w, r[1] / h);
            [a, b, r[2] - a * x0 - b * y0]
        };
        [row(self.rel[0]), row(self.rel[1])]
    }

    /// The fixture's box as the source sees it: the half-size of its image
    /// through T, axis by axis (the bounds of the turned box when the fit
    /// turns). `unit`'s `patternExtent` (D22).
    #[must_use]
    pub fn box_half_extent(&self) -> [f32; 2] {
        let half = |r: [f64; 3]| ((libm::fabs(r[0]) + libm::fabs(r[1])) / 2.0) as f32;
        [half(self.rel[0]), half(self.rel[1])]
    }

    /// Source units per fixture unit, for a length that has no direction (a
    /// lamp pitch): `√|det|` of T, which is the fit's one scale for contain
    /// and cover whatever the turn, and the area-preserving mean for fill.
    #[must_use]
    pub fn length_scale(&self) -> f64 {
        let [[a, b, _], [c, d, _]] = self.rel;
        let det = (a * d - b * c)
            / (f64::from(self.view_box.width()) * f64::from(self.view_box.height()));
        libm::sqrt(libm::fabs(det))
    }

    fn map_rel(&self, rel: [i32; 2]) -> [i32; 2] {
        [self.rows[0].apply(rel), self.rows[1].apply(rel)]
    }

    fn classify(&self, q: [i32; 2]) -> Sample {
        match self.extent_q16 {
            Some([x, y]) if !(x[0]..=x[1]).contains(&q[0]) || !(y[0]..=y[1]).contains(&q[1]) => {
                Sample::Outside(q)
            }
            _ => Sample::At(q),
        }
    }
}

/// One output axis of T in fixed point: `round((mu·u + mv·v + c) / 2^shift)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FixedRow {
    mu: i64,
    mv: i64,
    c: i64,
    shift: u32,
}

impl FixedRow {
    /// Quantize `q = r[0]·u + r[1]·v + r[2]` (u, v fractions of the box) for
    /// Q2.30 in and Q16.16 out.
    ///
    /// With `u = U / 2^30` and `q16 = q · 2^16`, a row is
    /// `(r0·2^(16+K)·U + r1·2^(16+K)·V + r2·2^(46+K)) / 2^(30+K)` for any `K`.
    /// `K` is the largest that keeps `|M| < 2^29` (so `|M·U| < 2^60`) and
    /// `|C| < 2^61`: the sum stays below 2^62.
    fn new(r: [f64; 3]) -> Self {
        let a_max = libm::fmax(libm::fabs(r[0]), libm::fabs(r[1]));
        // 2^(e) ≤ a_max < 2^(e+1); M = a_max·2^(16+K) < 2^29 ⇔ K ≤ 12 − e.
        let mut k = 12 - floor_log2(a_max).unwrap_or(0);
        // |C| = |r2|·2^(46+K) < 2^61 ⇔ K ≤ 14 − e_c.
        if let Some(e_c) = floor_log2(libm::fabs(r[2])) {
            k = k.min(14 - e_c);
        }
        // shift = 30 + K, kept in 1..=62.
        let k = k.clamp(-29, 32);
        let shift = (30 + k) as u32;
        let scale = libm::ldexp(1.0, 16 + k);
        Self {
            mu: libm::round(r[0] * scale) as i64,
            mv: libm::round(r[1] * scale) as i64,
            c: libm::round(r[2] * libm::ldexp(1.0, 46 + k)) as i64,
            shift,
        }
    }

    fn apply(&self, [u, v]: [i32; 2]) -> i32 {
        let sum = self.mu * i64::from(u) + self.mv * i64::from(v) + self.c;
        let rounded = (sum + (1i64 << (self.shift - 1))) >> self.shift;
        rounded.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

/// The affine over box-relative coordinates, in `f64`.
///
/// Place the product: size it by the mode (fixture units per product unit,
/// `sx`, `sy`), align the placed rectangle in the box, then zoom by `k` and
/// turn by `θ` about its centre `c`. Forward, a product offset `d` from the
/// product's centre lands at `c + R(θ)·k·(sx·dx, sy·f·dy)` in the box, where
/// `f` is −1 for a y-up product (the flip). T is the inverse.
fn relative_affine(view_box: &ViewBox, fit: &Fit, product: &ProductBox) -> [[f64; 3]; 2] {
    let (fw, fh) = (f64::from(view_box.width()), f64::from(view_box.height()));
    let (pw, ph) = (f64::from(product.width()), f64::from(product.height()));
    let (rx, ry) = (fw / pw, fh / ph);
    let (sx, sy) = match fit.mode {
        FitMode::Contain => {
            let s = libm::fmin(rx, ry);
            (s, s)
        }
        FitMode::Cover => {
            let s = libm::fmax(rx, ry);
            (s, s)
        }
        FitMode::Fill => (rx, ry),
    };
    let (placed_w, placed_h) = (pw * sx, ph * sy);
    // The placed product's centre, from the box's top-left corner.
    let cx = fit.align.x.offset(fw, placed_w) + placed_w / 2.0;
    let cy = fit.align.y.offset(fh, placed_h) + placed_h / 2.0;
    let k = f64::from(fit.scale);
    let (cos, sin) = turn(f64::from(fit.rotation_deg));
    let flip = if product.y_up() { -1.0 } else { 1.0 };
    let [qx, qy] = product.centre();

    // q_x = qx + (cos·(px − cx) + sin·(py − cy)) / (k·sx)
    // q_y = qy + flip·(−sin·(px − cx) + cos·(py − cy)) / (k·sy)
    // with px = u·fw, py = v·fh.
    let (kx, ky) = (k * sx, k * sy);
    [
        [
            cos * fw / kx,
            sin * fh / kx,
            qx - (cos * cx + sin * cy) / kx,
        ],
        [
            -flip * sin * fw / ky,
            flip * cos * fh / ky,
            qy - flip * (-sin * cx + cos * cy) / ky,
        ],
    ]
}

/// `(cos θ, sin θ)` for `θ` in degrees, exact at the quarter turns (a turn
/// of 90° must map an edge onto an edge, not a hair beside it).
fn turn(degrees: f64) -> (f64, f64) {
    let quarter = degrees / 90.0;
    if libm::trunc(quarter) == quarter {
        return match (quarter as i64).rem_euclid(4) {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            2 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
    }
    let radians = degrees * (core::f64::consts::PI / 180.0);
    (libm::cos(radians), libm::sin(radians))
}

/// `floor(log2(|x|))` for a finite non-zero `x`; `None` for zero.
fn floor_log2(x: f64) -> Option<i32> {
    if x == 0.0 || !x.is_finite() {
        return None;
    }
    let (_, exp) = libm::frexp(x);
    // frexp: x = m·2^exp with m in [0.5, 1).
    Some(exp - 1)
}

/// A box fraction as Q2.30, rounded to nearest and saturated (far outside
/// stays outside).
fn to_rel(fraction: f64) -> i32 {
    libm::round(fraction * f64::from(REL_ONE)).clamp(f64::from(i32::MIN), f64::from(i32::MAX))
        as i32
}

/// A product coordinate as Q16.16, rounded to nearest and saturated.
fn to_q16(value: f64) -> i32 {
    libm::round(value * f64::from(Q16_ONE)).clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

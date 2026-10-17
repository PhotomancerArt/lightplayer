//! A cubic Bézier: the one curve every arrow is.
//!
//! A straight arrow is a cubic too (its handles on the line), so placement
//! and trimming have one shape to deal with. Only the path a renderer gets
//! says `LineTo` for a straight one.

use crate::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cubic {
    pub start: Vec2,
    pub c1: Vec2,
    pub c2: Vec2,
    pub end: Vec2,
}

impl Cubic {
    pub fn new(start: Vec2, c1: Vec2, c2: Vec2, end: Vec2) -> Self {
        Self { start, c1, c2, end }
    }

    /// The straight line from `a` to `b`, as a cubic.
    pub fn line(a: Vec2, b: Vec2) -> Self {
        Self::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b)
    }

    /// The point at parameter `t` (0–1). Not arc length: see
    /// [`Polyline`](crate::Polyline) for that.
    pub fn point(&self, t: f64) -> Vec2 {
        let u = 1.0 - t;
        self.start * (u * u * u)
            + self.c1 * (3.0 * u * u * t)
            + self.c2 * (3.0 * u * t * t)
            + self.end * (t * t * t)
    }

    /// The two halves either side of `t`, each the same curve exactly
    /// (de Casteljau).
    pub fn split(&self, t: f64) -> (Cubic, Cubic) {
        let ab = self.start.lerp(self.c1, t);
        let bc = self.c1.lerp(self.c2, t);
        let cd = self.c2.lerp(self.end, t);
        let abc = ab.lerp(bc, t);
        let bcd = bc.lerp(cd, t);
        let p = abc.lerp(bcd, t);
        (
            Cubic::new(self.start, ab, abc, p),
            Cubic::new(p, bcd, cd, self.end),
        )
    }

    /// The part between parameters `t0` and `t1` (`t0 < t1`).
    pub fn between(&self, t0: f64, t1: f64) -> Cubic {
        if t1 <= 0.0 {
            return Cubic::line(self.start, self.start);
        }
        let (left, _) = self.split(t1);
        let (_, mid) = left.split((t0 / t1).clamp(0.0, 1.0));
        mid
    }

    /// The parameter, counting back from the end, where the curve is first
    /// `distance` (straight-line) from the end point; 0 when the whole curve
    /// is nearer than that. `from_start` counts from the start instead.
    pub fn param_at_distance(&self, distance: f64, from_start: bool) -> f64 {
        let anchor = if from_start { self.start } else { self.end };
        let at = |i: f64| if from_start { i } else { 1.0 - i };
        if distance <= 0.0 {
            return at(0.0);
        }
        const STEPS: usize = 64;
        let mut prev = 0.0;
        for k in 1..=STEPS {
            let i = k as f64 / STEPS as f64;
            if self.point(at(i)).distance(anchor) >= distance {
                // Bisect between the last step inside and this one.
                let (mut lo, mut hi) = (prev, i);
                for _ in 0..30 {
                    let mid = (lo + hi) / 2.0;
                    if self.point(at(mid)).distance(anchor) >= distance {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return at(hi);
            }
            prev = i;
        }
        at(1.0)
    }
}

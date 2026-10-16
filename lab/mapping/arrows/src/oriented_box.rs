//! A rectangle turned to an angle: the little box behind a label.
//!
//! A label lies along its line, so its box is rotated to the line's angle.
//! Placement asks two questions of boxes — does it touch that one, does it
//! touch that arrowhead — and both are the separating-axis test over convex
//! outlines, which is exact for rectangles and triangles.

use crate::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrientedBox {
    pub centre: Vec2,
    /// Along the box's own x axis: the way the text reads.
    pub width: f64,
    pub height: f64,
    /// Clockwise from +x, in degrees (y down, as SVG's `rotate`).
    pub angle_deg: f64,
}

impl OrientedBox {
    pub fn new(centre: Vec2, width: f64, height: f64, angle_deg: f64) -> Self {
        Self {
            centre,
            width,
            height,
            angle_deg,
        }
    }

    /// The unit vector the text reads along.
    pub fn axis(&self) -> Vec2 {
        Vec2::from_angle_deg(self.angle_deg)
    }

    /// The four corners, in order around the box.
    pub fn corners(&self) -> [Vec2; 4] {
        let u = self.axis() * (self.width / 2.0);
        let v = self.axis().perp() * (self.height / 2.0);
        let c = self.centre;
        [c - u - v, c + u - v, c + u + v, c - u + v]
    }

    /// The same box grown by `margin` on every side.
    pub fn inflated(&self, margin: f64) -> OrientedBox {
        OrientedBox {
            width: self.width + 2.0 * margin,
            height: self.height + 2.0 * margin,
            ..*self
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let d = p - self.centre;
        let u = self.axis();
        d.dot(u).abs() <= self.width / 2.0 && d.dot(u.perp()).abs() <= self.height / 2.0
    }

    /// The two boxes share any area (touching edges count).
    pub fn overlaps(&self, other: &OrientedBox) -> bool {
        convex_overlap(&self.corners(), &other.corners())
    }

    /// The box shares any area with a convex polygon (an arrowhead).
    pub fn overlaps_polygon(&self, polygon: &[Vec2]) -> bool {
        polygon.len() >= 2 && convex_overlap(&self.corners(), polygon)
    }

    pub fn bounds(&self) -> Rect {
        Rect::around(self.corners()).unwrap_or(Rect {
            min: self.centre,
            max: self.centre,
        })
    }
}

/// The separating-axis test: two convex outlines are apart exactly when
/// some edge normal of one of them separates their projections.
fn convex_overlap(a: &[Vec2], b: &[Vec2]) -> bool {
    for poly in [a, b] {
        for i in 0..poly.len() {
            let edge = poly[(i + 1) % poly.len()] - poly[i];
            let n = edge.perp();
            if n == Vec2::ZERO {
                continue;
            }
            let (amin, amax) = project(a, n);
            let (bmin, bmax) = project(b, n);
            if amax < bmin || bmax < amin {
                return false;
            }
        }
    }
    true
}

fn project(poly: &[Vec2], n: Vec2) -> (f64, f64) {
    poly.iter()
        .map(|p| p.dot(n))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
            (lo.min(d), hi.max(d))
        })
}

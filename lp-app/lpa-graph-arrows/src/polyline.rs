//! A curve as straight pieces, measured along its length.
//!
//! Label placement thinks in distance along the line ("12 units from the
//! start"), not in a Bézier's parameter, which bunches up where the curve
//! bends. This is that measuring stick: points, and the running length at
//! each.

use crate::Vec2;

#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    points: Vec<Vec2>,
    /// `lengths[i]`: distance along the line from the first point to
    /// `points[i]`.
    lengths: Vec<f64>,
}

impl Polyline {
    pub fn new(points: Vec<Vec2>) -> Self {
        let mut lengths = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                total += p.distance(points[i - 1]);
            }
            lengths.push(total);
        }
        Self { points, lengths }
    }

    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    pub fn length(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// The point `s` along the line (clamped to its ends).
    pub fn point_at(&self, s: f64) -> Vec2 {
        match self.piece_at(s) {
            Some((i, f)) => self.points[i].lerp(self.points[i + 1], f),
            None => self.points.first().copied().unwrap_or(Vec2::ZERO),
        }
    }

    /// The unit direction of travel `s` along the line.
    pub fn tangent_at(&self, s: f64) -> Vec2 {
        match self.piece_at(s) {
            Some((i, _)) => (self.points[i + 1] - self.points[i]).normalized(),
            None => Vec2::ZERO,
        }
    }

    /// The stretch from `a` to `b` along the line: its chord's two ends,
    /// and how far the line strays from that chord in between.
    pub fn chord(&self, a: f64, b: f64) -> (Vec2, Vec2, f64) {
        let pa = self.point_at(a);
        let pb = self.point_at(b);
        let dir = (pb - pa).normalized();
        let off = |p: Vec2| (p - pa).cross(dir).abs();
        let stray = self
            .points
            .iter()
            .zip(&self.lengths)
            .filter(|(_, l)| **l > a && **l < b)
            .map(|(p, _)| off(*p))
            .fold(0.0, f64::max);
        (pa, pb, stray)
    }

    /// The nearest distance from `p` to the line.
    pub fn distance_to(&self, p: Vec2) -> f64 {
        if self.points.len() == 1 {
            return self.points[0].distance(p);
        }
        self.points
            .windows(2)
            .map(|w| distance_to_segment(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min)
    }

    /// The piece holding `s`, skipping zero-length pieces, and how far
    /// along it (0–1).
    fn piece_at(&self, s: f64) -> Option<(usize, f64)> {
        if self.points.len() < 2 {
            return None;
        }
        let s = s.clamp(0.0, self.length());
        let last = self.points.len() - 2;
        let mut found = None;
        for i in 0..=last {
            let (l0, l1) = (self.lengths[i], self.lengths[i + 1]);
            if l1 > l0 {
                found = Some(i);
                if s <= l1 {
                    return Some((i, (s - l0) / (l1 - l0)));
                }
            }
        }
        found.map(|i| (i, 1.0))
    }
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

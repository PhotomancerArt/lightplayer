//! Where lamps land, in the product's own arithmetic.
//!
//! The lab keeps its numbers in f64, but a lamp's position is worked out
//! exactly the way the product's resolver works it out (`lpc-mapping`,
//! `map2d_resolve.rs`): authored numbers rounded to f32, f32 arithmetic in
//! the same order, trig through `libm`. So a lamp the lab draws is, bit for
//! bit, the lamp the device lights (fit-check §5.7), and the fit test can
//! say "equal", not "close". An f32 widens to f64 exactly, so nothing is
//! lost on the way back.
//!
//! The rules, each the product's:
//!
//! - **a path**: `count` lamps spread evenly along its *active* length — a
//!   gap segment is jumper wire and carries none — the first on its start,
//!   the last on its end. One lamp sits on its start. `reversed` walks it
//!   from its last point.
//! - **a polygon**: closed (the last point runs back to the first), lamp 0
//!   on its first point, one lamp every perimeter / count — no doubled
//!   corner, so a triangle of 9 lamps has 3 on each side.
//! - **a ring**: lamp 0 at 12 o'clock, clockwise in y-down space (or
//!   counter-clockwise), one every 360° / count.
//! - **a turn** ([`Rotation`]): about a centre, a positive angle clockwise —
//!   a repeat's instance `k` is its shape turned `k · 360° / count`.
//!
//! A shape the product would refuse (a zero-length path) still gets lamps
//! here, all on its first point, so a drawing in progress never vanishes.

use crate::geom::Vec2;

/// A point the way the product holds it.
pub type P32 = [f32; 2];

/// An authored point, rounded the way the product reads it.
pub fn p32(p: Vec2) -> P32 {
    [p.x as f32, p.y as f32]
}

/// A product point, widened exactly.
pub fn vec2(p: P32) -> Vec2 {
    Vec2::new(p[0] as f64, p[1] as f64)
}

/// A path's lamps: evenly along its active length, first to last (or last
/// to first when `reversed`). `gaps` are the indices of inert segments.
pub fn path_lamps(points: &[Vec2], count: u32, reversed: bool, gaps: &[u32]) -> Vec<Vec2> {
    if count == 0 || points.is_empty() {
        return Vec::new();
    }
    let mut pts: Vec<P32> = points.iter().map(|p| p32(*p)).collect();
    let inert = inert_segments(pts.len(), reversed, gaps);
    if reversed {
        pts.reverse();
    }
    let total = active_length(&pts, &inert);
    if pts.len() < 2 || total <= f32::EPSILON {
        return vec![vec2(pts[0]); count as usize];
    }
    if count == 1 {
        return vec![vec2(point_at_active_distance(&pts, &inert, 0.0))];
    }
    (0..count)
        .map(|lamp| {
            let distance = total * (lamp as f32 / (count - 1) as f32);
            vec2(point_at_active_distance(&pts, &inert, distance))
        })
        .collect()
}

/// A polygon's lamps: round its closed outline from its first point, one
/// every perimeter / count.
pub fn polygon_lamps(points: &[Vec2], count: u32) -> Vec<Vec2> {
    if count == 0 || points.is_empty() {
        return Vec::new();
    }
    let mut pts: Vec<P32> = points.iter().map(|p| p32(*p)).collect();
    pts.push(pts[0]);
    let perimeter = active_length(&pts, &[]);
    if points.len() < 2 || perimeter <= f32::EPSILON {
        return vec![vec2(pts[0]); count as usize];
    }
    (0..count)
        .map(|lamp| {
            let distance = perimeter * (lamp as f32 / count as f32);
            vec2(point_at_active_distance(&pts, &[], distance))
        })
        .collect()
}

/// One ring's lamps: lamp 0 at 12 o'clock, clockwise unless `ccw`.
pub fn ring_lamps(center: Vec2, radius: f64, count: u32, ccw: bool) -> Vec<Vec2> {
    if count == 0 {
        return Vec::new();
    }
    let center = p32(center);
    let radius = radius as f32;
    let sign = if ccw { -1.0f32 } else { 1.0f32 };
    let step = 360.0 / count as f32;
    (0..count)
        .map(|lamp| {
            let degrees = RING_START_DEG + sign * lamp as f32 * step;
            let radians = degrees * (core::f32::consts::PI / 180.0);
            vec2([
                center[0] + radius * libm::cosf(radians),
                center[1] + radius * libm::sinf(radians),
            ])
        })
        .collect()
}

/// Where a ring's lamp 0 sits: 12 o'clock in y-down space.
pub const RING_START_DEG: f32 = -90.0;

/// A turn about a point, worked out once and applied to each point — the
/// product's `Rotation2d`, so a repeat's instances land on the same floats
/// the device's do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation {
    center: P32,
    sin: f32,
    cos: f32,
}

impl Rotation {
    /// Instance `k` of a repeat of `count`: turned `k · 360° / count`.
    pub fn instance(center: Vec2, k: u32, count: u32) -> Rotation {
        let step = 360.0 / count.max(1) as f32;
        Rotation::about(center, k as f32 * step)
    }

    pub fn about(center: Vec2, degrees: f32) -> Rotation {
        let radians = degrees * (core::f32::consts::PI / 180.0);
        Rotation {
            center: p32(center),
            sin: libm::sinf(radians),
            cos: libm::cosf(radians),
        }
    }

    pub fn apply(&self, p: Vec2) -> Vec2 {
        let p = p32(p);
        let dx = p[0] - self.center[0];
        let dy = p[1] - self.center[1];
        vec2([
            self.center[0] + dx * self.cos - dy * self.sin,
            self.center[1] + dx * self.sin + dy * self.cos,
        ])
    }
}

/// Per segment, in walk order: is it jumper wire? A reversed path walks
/// its segments backwards, and the same physical segments stay inert.
fn inert_segments(points: usize, reversed: bool, gaps: &[u32]) -> Vec<bool> {
    let segments = points.saturating_sub(1);
    let mut inert = vec![false; segments];
    for gap in gaps {
        let i = *gap as usize;
        if i < segments {
            inert[if reversed { segments - 1 - i } else { i }] = true;
        }
    }
    inert
}

fn active_length(points: &[P32], inert: &[bool]) -> f32 {
    points
        .windows(2)
        .enumerate()
        .filter(|(i, _)| !is_inert(inert, *i))
        .map(|(_, w)| distance(w[0], w[1]))
        .sum()
}

/// The point `target` along the active length. Inert segments move the
/// walk without using any distance; float slop past the end lands on the
/// end of the last active segment.
fn point_at_active_distance(points: &[P32], inert: &[bool], target: f32) -> P32 {
    let mut remaining = target;
    let mut last_end = None;
    for (i, w) in points.windows(2).enumerate() {
        if is_inert(inert, i) {
            continue;
        }
        let (start, end) = (w[0], w[1]);
        let segment = distance(start, end);
        if segment <= f32::EPSILON {
            continue;
        }
        if remaining <= segment {
            let t = remaining / segment;
            return [
                start[0] + (end[0] - start[0]) * t,
                start[1] + (end[1] - start[1]) * t,
            ];
        }
        remaining -= segment;
        last_end = Some(end);
    }
    last_end.unwrap_or(points[0])
}

fn is_inert(inert: &[bool], i: usize) -> bool {
    inert.get(i).copied().unwrap_or(false)
}

fn distance(a: P32, b: P32) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    libm::sqrtf(dx * dx + dy * dy)
}

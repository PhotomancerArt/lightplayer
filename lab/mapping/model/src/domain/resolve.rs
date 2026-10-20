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
//! - **a grid**, **a ring set** and **a filled polygon**: the product's
//!   walks, copied step for step (each function says how).
//! - **a turn** ([`Rotation`]): about a centre, a positive angle clockwise —
//!   a repeat's instance `k` is its shape turned `k · 360° / count`.
//!
//! A shape the product would refuse (a zero-length path) still gets lamps
//! here, all on its first point, so a drawing in progress never vanishes.

use crate::domain::filled_polygon::FilledPolygon;
use crate::domain::geom::Vec2;
use crate::domain::grid::{Grid, Routing};
use crate::domain::ring_set::RingSet;

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

/// A grid's lamps: row by row from its start corner, each row back the
/// other way when it snakes. A grid with no columns or rows, or no pitch,
/// has none (the product refuses it).
pub fn grid_lamps(grid: &Grid) -> Vec<Vec2> {
    let pitch = grid.pitch as f32;
    if grid.cols == 0 || grid.rows == 0 || pitch <= 0.0 {
        return Vec::new();
    }
    let origin = p32(grid.origin);
    let flip_rows = grid.start_corner.from_bottom();
    let flip_cols = grid.start_corner.from_right();
    let mut out = Vec::with_capacity((grid.cols * grid.rows) as usize);
    for row_step in 0..grid.rows {
        let row = if flip_rows {
            grid.rows - 1 - row_step
        } else {
            row_step
        };
        let odd_row = grid.routing == Routing::Snake && row_step % 2 == 1;
        for col_step in 0..grid.cols {
            let col = if flip_cols == odd_row {
                col_step
            } else {
                grid.cols - 1 - col_step
            };
            out.push(vec2([
                origin[0] + col as f32 * pitch,
                origin[1] + row as f32 * pitch,
            ]));
        }
    }
    out
}

/// A ring set's lamps: ring after ring on one strand, outer first (or
/// inner first), each from `start_angle_deg`. A ring's count is its own
/// `counts` entry, or the outer count scaled by circumference,
/// `max(1, round(outer · r / R))`.
pub fn ring_set_lamps(set: &RingSet) -> Vec<Vec2> {
    let radius = set.radius as f32;
    if set.outer_lamps == 0 || radius <= 0.0 || set.rings == 0 {
        return Vec::new();
    }
    let center = p32(set.center);
    let mut rings: Vec<(f32, u32)> = (0..set.rings)
        .map(|k| {
            let r = radius * (set.rings - k) as f32 / set.rings as f32;
            let count = set
                .counts
                .get(k as usize)
                .copied()
                .filter(|c| *c > 0)
                .unwrap_or_else(|| {
                    (libm::roundf(set.outer_lamps as f32 * r / radius) as u32).max(1)
                });
            (r, count)
        })
        .collect();
    if set.inner_first {
        rings.reverse();
    }
    let sign = if set.reversed { -1.0f32 } else { 1.0f32 };
    let start = set.start_angle_deg as f32;
    let mut out = Vec::new();
    for (r, count) in rings {
        let step = 360.0 / count as f32;
        for lamp in 0..count {
            let degrees = start + sign * lamp as f32 * step;
            let radians = degrees * (core::f32::consts::PI / 180.0);
            out.push(vec2([
                center[0] + r * libm::cosf(radians),
                center[1] + r * libm::sinf(radians),
            ]));
        }
    }
    out
}

/// A filled polygon's lamps: the populated cells of its lattice, in wiring
/// order. The product's walk (`filled_polygon_cells`), step for step:
///
/// 1. the lattice is laid in a frame turned `−angle_deg` about the
///    outline's box centre, and the outline is carried into it;
/// 2. cell centres sit at `box.min + offset + (i + ½, j + ½) · pitch`, for
///    every cell inside the frame's box;
/// 3. a centre counts when it is inside the outline (even-odd) AND at
///    least `pitch / 1000` from every edge, so a centre on an edge is
///    always out;
/// 4. rows run top to bottom, cells left to right, the start corner flips
///    them, and a snake reverses every other row it *visits* (a row the
///    outline misses doesn't count).
///
/// Malformed input (fewer than three points, no pitch, a point that isn't
/// a number) has none, as the product refuses it.
pub fn filled_polygon_lamps(filled: &FilledPolygon) -> Vec<Vec2> {
    let pitch = filled.pitch as f32;
    if filled.points.len() < 3 || !(pitch > 0.0) || !pitch.is_finite() {
        return Vec::new();
    }
    let points: Vec<P32> = filled.points.iter().map(|p| p32(*p)).collect();
    if !points.iter().all(|p| p[0].is_finite() && p[1].is_finite()) {
        return Vec::new();
    }
    let angle = filled.angle_deg as f32;
    let offset = p32(filled.offset);
    let bbox = bounds(&points);
    let center = vec2([(bbox[0] + bbox[2]) * 0.5, (bbox[1] + bbox[3]) * 0.5]);
    let into_frame = Rotation::about(center, -angle);
    let out_of_frame = Rotation::about(center, angle);
    let outline: Vec<P32> = points
        .iter()
        .map(|p| p32(into_frame.apply(vec2(*p))))
        .collect();
    let [min_x, min_y, max_x, max_y] = bounds(&outline);
    let base_x = min_x + offset[0];
    let base_y = min_y + offset[1];
    let (first_col, last_col) = index_span(min_x, max_x, base_x, pitch);
    let (first_row, last_row) = index_span(min_y, max_y, base_y, pitch);
    if last_col < first_col || last_row < first_row {
        return Vec::new();
    }
    let epsilon = pitch * 1e-3;
    let mut rows: Vec<Vec<P32>> = Vec::new();
    for row in first_row..=last_row {
        let y = base_y + (row as f32 + 0.5) * pitch;
        let cells: Vec<P32> = (first_col..=last_col)
            .map(|col| [base_x + (col as f32 + 0.5) * pitch, y])
            .filter(|c| cell_is_populated(*c, &outline, epsilon))
            .collect();
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    let flip_rows = filled.start_corner.from_bottom();
    let flip_cols = filled.start_corner.from_right();
    let mut out = Vec::new();
    for row_step in 0..rows.len() {
        let row = if flip_rows {
            rows.len() - 1 - row_step
        } else {
            row_step
        };
        let odd_row = filled.routing == Routing::Snake && row_step % 2 == 1;
        let forward = flip_cols == odd_row;
        let cells = &rows[row];
        for col_step in 0..cells.len() {
            let cell = if forward {
                cells[col_step]
            } else {
                cells[cells.len() - 1 - col_step]
            };
            out.push(out_of_frame.apply(vec2(cell)));
        }
    }
    out
}

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

/// `[min_x, min_y, max_x, max_y]` over a non-empty point list.
fn bounds(points: &[P32]) -> [f32; 4] {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in points {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

/// The cells whose centres fall in `[low, high]`, for a lattice anchored at
/// `base`, one `pitch` apart.
fn index_span(low: f32, high: f32, base: f32, pitch: f32) -> (i32, i32) {
    let first = libm::ceilf((low - base) / pitch - 0.5);
    let last = libm::floorf((high - base) / pitch - 0.5);
    (first as i32, last as i32)
}

/// Inside the outline, and no closer than `epsilon` to any edge.
fn cell_is_populated(center: P32, outline: &[P32], epsilon: f32) -> bool {
    if !inside_outline(center, outline) {
        return false;
    }
    let mut previous = outline[outline.len() - 1];
    for p in outline {
        if distance_to_segment(center, previous, *p) < epsilon {
            return false;
        }
        previous = *p;
    }
    true
}

/// Even-odd, against the closed outline.
fn inside_outline(point: P32, outline: &[P32]) -> bool {
    let mut inside = false;
    let mut previous = outline[outline.len() - 1];
    for current in outline {
        if (previous[1] > point[1]) != (current[1] > point[1]) {
            let t = (point[1] - previous[1]) / (current[1] - previous[1]);
            if point[0] < previous[0] + t * (current[0] - previous[0]) {
                inside = !inside;
            }
        }
        previous = *current;
    }
    inside
}

fn distance_to_segment(point: P32, start: P32, end: P32) -> f32 {
    let vx = end[0] - start[0];
    let vy = end[1] - start[1];
    let length_squared = vx * vx + vy * vy;
    let t = if length_squared <= 0.0 {
        0.0
    } else {
        (((point[0] - start[0]) * vx + (point[1] - start[1]) * vy) / length_squared).clamp(0.0, 1.0)
    };
    distance(point, [start[0] + t * vx, start[1] + t * vy])
}

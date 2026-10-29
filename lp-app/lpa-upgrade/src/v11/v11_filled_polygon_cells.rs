//! A filled polygon's lattice cells — the one shape whose lamp count is
//! derived from its geometry.
//!
//! **Copied verbatim** from `lp-core/lpc-mapping/src/map2d_resolve.rs`
//! (`filled_polygon_cells`, `index_span`, `cell_is_populated`,
//! `point_inside_outline`, `distance_to_segment`, `point_bounds`,
//! `Rotation2d::about`/`apply`, `distance`) at origin/main
//! b9f862fb20a9832ed59fc02a1b9f9215f1b2ef0e (last changed in
//! aa113b4bb11a31b81b80f819a0a5acdf3703bfdc). Only the shape type is this
//! crate's own (fields read from JSON, not a typed model). The trig is
//! `libm`'s, as the source's is: a lattice turned by a nonzero `angle_deg`
//! must land on the resolver's exact floats or a cell on the inclusion
//! border could count differently. Never edit the arithmetic; a change
//! here is a different count for somebody's board.

/// The filled polygon's authored fields (`FilledPolygonShape`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FilledPolygon {
    pub(crate) points: Vec<[f32; 2]>,
    pub(crate) pitch: f32,
    pub(crate) angle_deg: f32,
    pub(crate) origin: [f32; 2],
}

/// How many cells `filled` populates — `filled_polygon_cells(filled).len()`.
///
/// The routing walk that orders the cells cannot change their number, so it
/// is not copied: positions are never computed here.
pub(crate) fn filled_polygon_cell_count(filled: &FilledPolygon) -> u32 {
    let pitch = filled.pitch;
    if filled.points.len() < 3 || !(pitch > 0.0) || !pitch.is_finite() {
        return 0;
    }
    if !filled
        .points
        .iter()
        .all(|point| point[0].is_finite() && point[1].is_finite())
    {
        return 0;
    }

    // 1. The lattice frame. `-angle_deg` enters it, `+angle_deg` leaves.
    let doc_bbox = point_bounds(&filled.points);
    let center = [
        (doc_bbox[0] + doc_bbox[2]) * 0.5,
        (doc_bbox[1] + doc_bbox[3]) * 0.5,
    ];
    let into_frame = Rotation2d::about(center, -filled.angle_deg);
    let outline: Vec<[f32; 2]> = filled
        .points
        .iter()
        .map(|point| into_frame.apply(*point))
        .collect();
    let [min_x, min_y, max_x, max_y] = point_bounds(&outline);

    // 2. Cell indices whose centers land inside the frame bbox.
    let base_x = min_x + filled.origin[0];
    let base_y = min_y + filled.origin[1];
    let (first_col, last_col) = index_span(min_x, max_x, base_x, pitch);
    let (first_row, last_row) = index_span(min_y, max_y, base_y, pitch);
    if last_col < first_col || last_row < first_row {
        return 0;
    }

    // 3. Populated cells.
    let epsilon = pitch * 1e-3;
    let mut count = 0u32;
    for row in first_row..=last_row {
        let y = base_y + (row as f32 + 0.5) * pitch;
        for col in first_col..=last_col {
            let center = [base_x + (col as f32 + 0.5) * pitch, y];
            if cell_is_populated(center, &outline, epsilon) {
                count += 1;
            }
        }
    }
    count
}

/// The inclusive index span of lattice lines whose centers fall in
/// `[low, high]`, for a lattice anchored at `base` with spacing `pitch`.
fn index_span(low: f32, high: f32, base: f32, pitch: f32) -> (i32, i32) {
    let first = libm::ceilf((low - base) / pitch - 0.5);
    let last = libm::floorf((high - base) / pitch - 0.5);
    (first as i32, last as i32)
}

/// Is a lattice cell center populated? Inside the outline, and no closer than
/// `epsilon` to any of its segments.
fn cell_is_populated(center: [f32; 2], outline: &[[f32; 2]], epsilon: f32) -> bool {
    if !point_inside_outline(center, outline) {
        return false;
    }
    let mut previous = outline[outline.len() - 1];
    for point in outline {
        if distance_to_segment(center, previous, *point) < epsilon {
            return false;
        }
        previous = *point;
    }
    true
}

/// Even-odd ray cast against a closed outline (the last point joins the
/// first).
fn point_inside_outline(point: [f32; 2], outline: &[[f32; 2]]) -> bool {
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

/// Distance from a point to a segment: project, clamp to the segment, measure.
fn distance_to_segment(point: [f32; 2], start: [f32; 2], end: [f32; 2]) -> f32 {
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

/// `[min_x, min_y, max_x, max_y]` over a non-empty point list.
fn point_bounds(points: &[[f32; 2]]) -> [f32; 4] {
    let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for point in points {
        bounds[0] = bounds[0].min(point[0]);
        bounds[1] = bounds[1].min(point[1]);
        bounds[2] = bounds[2].max(point[0]);
        bounds[3] = bounds[3].max(point[1]);
    }
    bounds
}

/// A rotation about a point, precomputed once and applied per point.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rotation2d {
    center: [f32; 2],
    sin: f32,
    cos: f32,
}

impl Rotation2d {
    fn about(center: [f32; 2], degrees: f32) -> Self {
        let radians = degrees * (core::f32::consts::PI / 180.0);
        Self {
            center,
            sin: libm::sinf(radians),
            cos: libm::cosf(radians),
        }
    }

    fn apply(&self, point: [f32; 2]) -> [f32; 2] {
        let dx = point[0] - self.center[0];
        let dy = point[1] - self.center[1];
        [
            self.center[0] + dx * self.cos - dy * self.sin,
            self.center[1] + dx * self.sin + dy * self.cos,
        ]
    }
}

/// Euclidean distance (`libm::sqrtf`, as the source).
pub(crate) fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    libm::sqrtf(dx * dx + dy * dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolver's own hand-computed walk (`map2d_resolve.rs`,
    /// `a_filled_polygon_snakes_its_populated_cells_row_by_row`): a right
    /// triangle `(0,0) (40,0) (0,40)` on a 10-unit lattice populates six
    /// cells — the four centers on the hypotenuse are ε-excluded.
    #[test]
    fn a_right_triangle_on_a_10_unit_lattice_counts_six_cells() {
        let triangle = FilledPolygon {
            points: vec![[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]],
            pitch: 10.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
        };
        assert_eq!(filled_polygon_cell_count(&triangle), 6);
    }

    /// A pitch coarser than the outline places nothing.
    #[test]
    fn a_pitch_coarser_than_the_outline_counts_no_cells() {
        let small = FilledPolygon {
            points: vec![[0.0, 0.0], [4.0, 0.0], [2.0, 3.0]],
            pitch: 16.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
        };
        assert_eq!(filled_polygon_cell_count(&small), 0);
    }
}

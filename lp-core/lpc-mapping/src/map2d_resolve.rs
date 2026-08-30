//! The single deterministic resolver: document → ordered lamps.
//!
//! Wiring order is primary: lamps are numbered end-to-end across objects in
//! document order, and that zero-based index is a lamp's one address here.
//!
//! Manual patching layers on top and never touches the wiring order: it lives
//! in the fixture's own patch document ([`crate::PatchDoc`]), addressing runs
//! of lamps by their position in THIS order. Placing a run on an output's
//! port is the patch/output layer's job — the resolver deliberately derives
//! no wire-side addresses, and "universe"/"channel" (real 512-limited DMX
//! terms) stay out of its vocabulary entirely (D45).

use alloc::string::ToString;
use alloc::vec::Vec;

use crate::map2d_doc::{
    FilledPolygonShape, GridCorner, GridRouting, GridShape, Map2dDoc, Map2dObject, Map2dShape,
    PathShape, PolygonShape, RepeatShape, RingDir, RingOrder, RingShape,
};
use crate::map2d_error::Map2dError;

/// One resolved lamp in doc space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedLamp {
    /// Zero-based wiring-order index across the whole document.
    pub index: u32,
    /// Wiring-order index of the owning object in `doc.objects`.
    pub object: u32,
    /// Position in doc space (fit to a render target separately).
    pub pos: [f32; 2],
}

/// One contiguous physical strand of lamps, and the document object it came
/// from.
///
/// Usually one span per object — but a [`RepeatShape`] object emits **one span
/// per instance**, all carrying the same `object` index, because each instance
/// is its own strand of wire. Consumers that mean "this object's whole lamp
/// range" want [`ResolvedMap2d::object_span`]; consumers that mean "the
/// physical runs" (the fixture's honest spans, the output face's strip
/// boundaries) want the span list itself; the Mapping editor's wiring
/// annotations keep the AUTHORED grain — each object's first span only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectSpan {
    pub object: u32,
    /// Zero-based wiring index of the strand's first lamp.
    pub start: u32,
    pub count: u32,
}

/// A fully resolved document.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMap2d {
    pub lamps: Vec<ResolvedLamp>,
    pub spans: Vec<ObjectSpan>,
}

impl ResolvedMap2d {
    pub fn positions(&self) -> Vec<[f32; 2]> {
        self.lamps.iter().map(|lamp| lamp.pos).collect()
    }

    /// The whole lamp range one document object occupies, its strands merged.
    ///
    /// An object's lamps are always contiguous in wiring order, so merging is
    /// exact: `start` is its first strand's start and `count` the sum. This is
    /// what a per-object read-path wants (rail rows, the properties popover,
    /// expand) now that [`ObjectSpan`] means *strand*, not *object* — indexing
    /// `spans[object]` is only correct while no document repeats.
    pub fn object_span(&self, object: u32) -> Option<ObjectSpan> {
        let mut start: Option<u32> = None;
        let mut count = 0;
        for span in self.spans.iter().filter(|span| span.object == object) {
            start = Some(start.map_or(span.start, |first: u32| first.min(span.start)));
            count += span.count;
        }
        start.map(|start| ObjectSpan {
            object,
            start,
            count,
        })
    }
}

/// Resolve a document into its ordered lamp list.
pub fn resolve(doc: &Map2dDoc) -> Result<ResolvedMap2d, Map2dError> {
    let mut lamps = Vec::new();
    let mut spans = Vec::new();
    for (object_index, object) in doc.objects.iter().enumerate() {
        let object_index = object_index as u32;
        let start = lamps.len() as u32;
        let invalid = |reason: &str| Map2dError::InvalidObject {
            object: object_index,
            name: object.name.clone(),
            reason: reason.to_string(),
        };
        let ShapeLamps { positions, strands } = resolve_shape(&object.shape, &invalid)?;
        for pos in positions {
            let index = lamps.len() as u32;
            lamps.push(ResolvedLamp {
                index,
                object: object_index,
                pos,
            });
        }
        // One span per strand, all naming this object. A plain shape has
        // exactly one; a repeat has one per instance.
        let mut strand_start = start;
        for count in strands {
            spans.push(ObjectSpan {
                object: object_index,
                start: strand_start,
                count,
            });
            strand_start += count;
        }
    }
    Ok(ResolvedMap2d { lamps, spans })
}

/// What one shape resolves to: its lamp positions in wiring order, plus how
/// those positions divide into physical strands.
struct ShapeLamps {
    positions: Vec<[f32; 2]>,
    /// Lamp count of each strand, in wiring order; sums to `positions.len()`.
    strands: Vec<u32>,
}

/// Resolve one shape, recursing through [`Map2dShape::Repeat`].
///
/// Every leaf shape is a single strand. Only `Repeat` multiplies them, and it
/// does so by resolving its inner shape once and rotating the result, so a
/// nested repeat's strand list is the outer count times the inner list.
fn resolve_shape(
    shape: &Map2dShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<ShapeLamps, Map2dError> {
    let positions = match shape {
        Map2dShape::Grid(grid) => resolve_grid(grid, invalid)?,
        Map2dShape::Ring(ring) => resolve_ring(ring, invalid)?,
        Map2dShape::Path(path) => resolve_path(path, invalid)?,
        Map2dShape::Polygon(polygon) => resolve_polygon(polygon, invalid)?,
        Map2dShape::FilledPolygon(filled) => resolve_filled_polygon(filled, invalid)?,
        Map2dShape::Repeat(repeat) => return resolve_repeat(repeat, invalid),
    };
    let strands = alloc::vec![positions.len() as u32];
    Ok(ShapeLamps { positions, strands })
}

/// `count` rotated copies of the inner shape, wired instance by instance.
///
/// Instance `k` is the inner shape rotated `k * 360 / count` degrees about
/// `center` (screen coordinates, y-down, same trig as [`resolve_ring`]);
/// instance 0 comes out bit-identical to the unrotated inner shape because
/// `sinf(0)`/`cosf(0)` are exact. All of instance 0's lamps precede all of
/// instance 1's — instance `k` is physical strand `k`.
fn resolve_repeat(
    repeat: &RepeatShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<ShapeLamps, Map2dError> {
    if repeat.count == 0 {
        return Err(invalid("repeat count must be at least 1"));
    }
    let inner = resolve_shape(&repeat.shape, invalid)?;
    let mut positions = Vec::with_capacity(inner.positions.len() * repeat.count as usize);
    let mut strands = Vec::with_capacity(inner.strands.len() * repeat.count as usize);
    for instance in 0..repeat.count {
        let rotation = Rotation2d::about(repeat.center, repeat.instance_degrees(instance));
        for position in &inner.positions {
            positions.push(rotation.apply(*position));
        }
        strands.extend_from_slice(&inner.strands);
    }
    Ok(ShapeLamps { positions, strands })
}

/// A rotation about a point, precomputed once and applied per point.
///
/// This is the *only* place a repeat's turn is computed, so an editor that
/// rotates authored geometry (expanding a repeat into independent objects,
/// drawing ghost instance outlines) gets bit-identical results to the
/// resolver instead of its own re-derived trig. Screen coordinates, y-down:
/// a positive angle turns clockwise, matching [`resolve_ring`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation2d {
    center: [f32; 2],
    sin: f32,
    cos: f32,
}

impl Rotation2d {
    #[must_use]
    pub fn about(center: [f32; 2], degrees: f32) -> Self {
        let radians = degrees * (core::f32::consts::PI / 180.0);
        Self {
            center,
            sin: libm::sinf(radians),
            cos: libm::cosf(radians),
        }
    }

    #[must_use]
    pub fn apply(&self, point: [f32; 2]) -> [f32; 2] {
        let dx = point[0] - self.center[0];
        let dy = point[1] - self.center[1];
        [
            self.center[0] + dx * self.cos - dy * self.sin,
            self.center[1] + dx * self.sin + dy * self.cos,
        ]
    }
}

fn resolve_grid(
    grid: &GridShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<Vec<[f32; 2]>, Map2dError> {
    if grid.cols == 0 || grid.rows == 0 {
        return Err(invalid("grid needs at least one column and one row"));
    }
    if grid.pitch <= 0.0 {
        return Err(invalid("grid pitch must be positive"));
    }

    let flip_rows = matches!(grid.start_corner, GridCorner::Bl | GridCorner::Br);
    let flip_cols = matches!(grid.start_corner, GridCorner::Tr | GridCorner::Br);
    let mut positions = Vec::with_capacity((grid.cols * grid.rows) as usize);
    for row_step in 0..grid.rows {
        let row = if flip_rows {
            grid.rows - 1 - row_step
        } else {
            row_step
        };
        let odd_row = grid.routing == GridRouting::Snake && row_step % 2 == 1;
        for col_step in 0..grid.cols {
            let forward = flip_cols == odd_row;
            let col = if forward {
                col_step
            } else {
                grid.cols - 1 - col_step
            };
            positions.push([
                grid.origin[0] + col as f32 * grid.pitch,
                grid.origin[1] + row as f32 * grid.pitch,
            ]);
        }
    }
    Ok(positions)
}

fn resolve_ring(
    ring: &RingShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<Vec<[f32; 2]>, Map2dError> {
    if ring.outer_count == 0 {
        return Err(invalid("ring outer_count must be at least 1"));
    }
    if ring.radius <= 0.0 {
        return Err(invalid("ring radius must be positive"));
    }
    if ring.rings == 0 {
        return Err(invalid("ring needs at least one ring"));
    }
    // Rings auto-space evenly from the outer radius toward the center
    // (never reaching zero); per-ring counts come from the outer→inner
    // `counts` overrides, falling back to circumference-derived.
    let mut rings_outer_first: Vec<(f32, u32)> = Vec::with_capacity(ring.rings as usize);
    for ring_index in 0..ring.rings {
        let radius = ring.radius * (ring.rings - ring_index) as f32 / ring.rings as f32;
        let count = ring
            .counts
            .get(ring_index as usize)
            .copied()
            .filter(|count| *count > 0)
            .unwrap_or_else(|| derived_ring_count(ring.outer_count, radius, ring.radius));
        rings_outer_first.push((radius, count));
    }
    if ring.order == RingOrder::InnerFirst {
        rings_outer_first.reverse();
    }

    let sign = match ring.dir {
        RingDir::Cw => 1.0f32,
        RingDir::Ccw => -1.0f32,
    };
    let mut positions = Vec::new();
    for (radius, count) in rings_outer_first {
        let step = 360.0 / count as f32;
        for lamp in 0..count {
            let degrees = ring.start_angle_deg + sign * lamp as f32 * step;
            let radians = degrees * (core::f32::consts::PI / 180.0);
            positions.push([
                ring.center[0] + radius * libm::cosf(radians),
                ring.center[1] + radius * libm::sinf(radians),
            ]);
        }
    }
    Ok(positions)
}

/// Inner ring counts scale with circumference: `max(1, round(outer * r / R))`.
fn derived_ring_count(outer_count: u32, radius: f32, outer_radius: f32) -> u32 {
    let scaled = libm::roundf(outer_count as f32 * radius / outer_radius);
    (scaled as u32).max(1)
}

fn resolve_path(
    path: &PathShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<Vec<[f32; 2]>, Map2dError> {
    if path.count == 0 {
        return Err(invalid("path count must be at least 1"));
    }
    if path.points.len() < 2 {
        return Err(invalid("path needs at least 2 points"));
    }
    let mut points = path.points.clone();
    let inert = inert_segments(path);
    if path.reversed {
        points.reverse();
    }

    // Lamps are distributed over the ACTIVE length only: a jumper wire between
    // two lit runs carries no lamps, and the strip's pitch stays uniform
    // across the whole channel (fixed-pitch strip cut at a hub and jumpered).
    let total_active = active_length(&points, &inert);
    if total_active <= f32::EPSILON {
        return Err(invalid(if path.gaps.is_empty() {
            "path has zero length"
        } else {
            "path has no active length (every segment is a gap)"
        }));
    }

    let mut positions = Vec::with_capacity(path.count as usize);
    if path.count == 1 {
        positions.push(point_at_active_distance(&points, &inert, 0.0));
        return Ok(positions);
    }
    for lamp in 0..path.count {
        let distance = total_active * (lamp as f32 / (path.count - 1) as f32);
        positions.push(point_at_active_distance(&points, &inert, distance));
    }
    Ok(positions)
}

/// The lamp offsets INSIDE a path's strand where a jumper wire breaks it.
///
/// A gap segment carries no lamps and no active length, so the lamps either
/// side of one are physically separate runs — a renderer drawing the strand
/// as a BODY must not bridge them, the way it must not bridge two objects.
/// Offsets are into the path's own lamp list (`0 < offset < count`),
/// ascending and deduplicated; a gap before the first lamp or after the last
/// breaks nothing and is omitted.
///
/// Lives beside [`resolve_path`] on purpose: it walks the same reversed and
/// inert orientation over the same lengths, so the answer cannot drift from
/// where the lamps actually land.
#[must_use]
pub fn path_gap_breaks(path: &PathShape) -> Vec<u32> {
    if path.gaps.is_empty() || path.count < 2 || path.points.len() < 2 {
        return Vec::new();
    }
    let mut points = path.points.clone();
    let inert = inert_segments(path);
    if path.reversed {
        points.reverse();
    }
    let total = active_length(&points, &inert);
    if total <= f32::EPSILON {
        return Vec::new();
    }
    // One walk: every inert segment sits at the active distance walked so
    // far (it consumes none of its own).
    let mut boundaries = Vec::new();
    let mut walked = 0.0;
    for (index, pair) in points.windows(2).enumerate() {
        if is_inert(&inert, index) {
            boundaries.push(walked);
        } else {
            walked += distance(pair[0], pair[1]);
        }
    }
    // Lamp `n` sits at `total * n / (count - 1)`, so the break falls before
    // the first lamp PAST the boundary. A lamp exactly on it sits at the
    // gap's near end and stays with the run before.
    let mut breaks = Vec::new();
    for boundary in boundaries {
        // A jumper before the first lamp or after the last one breaks
        // nothing: every lamp is on one side of it.
        if boundary <= f32::EPSILON || boundary >= total - f32::EPSILON {
            continue;
        }
        let past = (0..path.count)
            .find(|lamp| total * (*lamp as f32 / (path.count - 1) as f32) > boundary)
            .unwrap_or(0);
        if past > 0 {
            breaks.push(past);
        }
    }
    breaks.sort_unstable();
    breaks.dedup();
    breaks
}

/// `count` lamps evenly spaced along a closed outline's perimeter.
///
/// The outline closes implicitly (last point → first point is a real
/// segment), the walk wraps instead of doubling an endpoint, and lamp 0
/// sits exactly on `points[0]` — so spacing is `perimeter / count`, and a
/// triangle of 9 lamps over equal sides carries exactly 3 per side with
/// lamps 0/3/6 on the corners. That per-side regularity is the polygon's
/// intrinsic rotation stride ([`shape_stride`]).
fn resolve_polygon(
    polygon: &PolygonShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<Vec<[f32; 2]>, Map2dError> {
    if polygon.count == 0 {
        return Err(invalid("polygon count must be at least 1"));
    }
    if polygon.points.len() < 3 {
        return Err(invalid("polygon needs at least 3 points"));
    }
    let mut points = polygon.points.clone();
    points.push(polygon.points[0]);
    let perimeter = active_length(&points, &[]);
    if perimeter <= f32::EPSILON {
        return Err(invalid("polygon has zero perimeter"));
    }
    let positions = (0..polygon.count)
        .map(|lamp| {
            let distance = perimeter * (lamp as f32 / polygon.count as f32);
            point_at_active_distance(&points, &[], distance)
        })
        .collect();
    Ok(positions)
}

/// A polygon outline filled with a lamp lattice — the shaped-matrix
/// primitive, and the first shape whose lamp count is DERIVED rather than
/// authored.
///
/// The walk itself lives in [`filled_polygon_cells`], shared verbatim with
/// [`shape_lamp_count`]; this wrapper only names the ways it can come back
/// empty, as the same object-naming error every other shape gives. It checks
/// the malformed cases FIRST so each gets its own reason — the walk itself
/// answers "empty" to all of them alike.
fn resolve_filled_polygon(
    filled: &FilledPolygonShape,
    invalid: &impl Fn(&str) -> Map2dError,
) -> Result<Vec<[f32; 2]>, Map2dError> {
    if filled.points.len() < 3 {
        return Err(invalid("filled polygon needs at least 3 points"));
    }
    // Written as a negated comparison on purpose: NaN fails it too.
    if !(filled.pitch > 0.0) || !filled.pitch.is_finite() {
        return Err(invalid("filled polygon pitch must be positive"));
    }
    if !filled
        .points
        .iter()
        .all(|point| point[0].is_finite() && point[1].is_finite())
    {
        return Err(invalid("filled polygon outline has a non-finite point"));
    }
    let cells = filled_polygon_cells(filled);
    if cells.is_empty() {
        // A pitch coarser than the outline (or an outline with no interior)
        // places nothing. An object that cannot light a single lamp is a
        // load-time error naming the object, exactly like a zero-perimeter
        // polygon — never a silently empty span.
        return Err(invalid("filled polygon contains no lattice cells"));
    }
    Ok(cells)
}

/// The populated cell centers of a filled polygon, in wiring order.
///
/// Shared **verbatim** by the resolver and [`shape_lamp_count`] so the derived
/// count can never drift from the resolved lamps — with no authored `count` to
/// cross-check against, agreement between the two has to come from there being
/// only one walk, not from two derivations that ought to match.
///
/// Returns empty for input the resolver rejects (fewer than 3 points, a
/// non-positive or non-finite pitch, non-finite vertices); the resolver checks
/// those first so it can name each one.
///
/// The walk, in order:
///
/// 1. **Lattice frame.** The lattice is laid out in a frame rotated by
///    `-angle_deg` about the outline's doc-space bbox center, using the shared
///    [`Rotation2d`] so an editor previewing the lattice lands on the resolver's
///    exact floats instead of its own re-derived trig. The outline is carried
///    into that frame; everything below happens there, and only the finished
///    centers come back out.
/// 2. **Cells.** Centers sit at `frame_bbox.min + origin + [(i + 0.5) * pitch,
///    (j + 0.5) * pitch]` for every `i`/`j` landing inside the frame bbox. The
///    half-cell offset is what makes a pitch-aligned outline populate
///    symmetrically rather than crowding one edge; `origin` slides the whole
///    lattice for the cases where the board wants a different phase.
/// 3. **Inclusion** — the ε-inset (`ε = pitch * 1e-3`): a center counts iff it
///    is inside the outline (even-odd ray cast) AND at least ε from every
///    outline segment. The inset is doing two jobs. It is the determinism
///    tie-break — a center landing exactly on an edge is *always* excluded, so
///    the count never depends on which side a float rounds to — and it is the
///    drag damper: while a vertex is being dragged, cells near the edge leave
///    and return across a band rather than blinking on the exact crossing.
/// 4. **Routing.** Rows run top-to-bottom, cells left-to-right, then
///    `start_corner` flips rows/columns and `Snake` alternates direction —
///    the same walk as [`resolve_grid`], but over populated cells only.
///    Snake parity counts **visited** rows (rows with at least one populated
///    cell): a serpentine chain on a shaped board reverses at the end of each
///    row of *copper*, so a row the outline skips entirely does not flip the
///    direction of the next one.
#[must_use]
pub fn filled_polygon_cells(filled: &FilledPolygonShape) -> Vec<[f32; 2]> {
    let pitch = filled.pitch;
    if filled.points.len() < 3 || !(pitch > 0.0) || !pitch.is_finite() {
        return Vec::new();
    }
    if !filled
        .points
        .iter()
        .all(|point| point[0].is_finite() && point[1].is_finite())
    {
        return Vec::new();
    }

    // 1. The lattice frame. `-angle_deg` enters it, `+angle_deg` leaves.
    let doc_bbox = point_bounds(&filled.points);
    let center = [
        (doc_bbox[0] + doc_bbox[2]) * 0.5,
        (doc_bbox[1] + doc_bbox[3]) * 0.5,
    ];
    let into_frame = Rotation2d::about(center, -filled.angle_deg);
    let out_of_frame = Rotation2d::about(center, filled.angle_deg);
    let outline: Vec<[f32; 2]> = filled
        .points
        .iter()
        .map(|point| into_frame.apply(*point))
        .collect();
    let [min_x, min_y, max_x, max_y] = point_bounds(&outline);

    // 2. Cell indices whose centers land inside the frame bbox. Solving
    // `min <= base + (n + 0.5) * pitch <= max` for `n` keeps the span
    // proportional to bbox/pitch however far `origin` slides the lattice.
    let base_x = min_x + filled.origin[0];
    let base_y = min_y + filled.origin[1];
    let (first_col, last_col) = index_span(min_x, max_x, base_x, pitch);
    let (first_row, last_row) = index_span(min_y, max_y, base_y, pitch);
    if last_col < first_col || last_row < first_row {
        return Vec::new();
    }

    // 3. Populated cells, gathered row by row so the routing walk below can
    // reverse a row without re-testing inclusion.
    let epsilon = pitch * 1e-3;
    let mut rows: Vec<Vec<[f32; 2]>> = Vec::new();
    for row in first_row..=last_row {
        let y = base_y + (row as f32 + 0.5) * pitch;
        let mut cells = Vec::new();
        for col in first_col..=last_col {
            let center = [base_x + (col as f32 + 0.5) * pitch, y];
            if cell_is_populated(center, &outline, epsilon) {
                cells.push(center);
            }
        }
        // Only VISITED rows are recorded — an empty row is not a row the
        // chain ever reaches, so it must not consume a snake parity step.
        if !cells.is_empty() {
            rows.push(cells);
        }
    }

    // 4. The routing walk, mirroring `resolve_grid`: the start corner flips
    // the row order and the column order, and snake alternates on the
    // traversal index.
    let flip_rows = matches!(filled.start_corner, GridCorner::Bl | GridCorner::Br);
    let flip_cols = matches!(filled.start_corner, GridCorner::Tr | GridCorner::Br);
    let mut positions = Vec::new();
    for row_step in 0..rows.len() {
        let row = if flip_rows {
            rows.len() - 1 - row_step
        } else {
            row_step
        };
        let odd_row = filled.routing == GridRouting::Snake && row_step % 2 == 1;
        let forward = flip_cols == odd_row;
        let cells = &rows[row];
        for col_step in 0..cells.len() {
            let cell = if forward {
                cells[col_step]
            } else {
                cells[cells.len() - 1 - col_step]
            };
            positions.push(out_of_frame.apply(cell));
        }
    }
    positions
}

/// The inclusive index span of lattice lines whose centers fall in
/// `[low, high]`, for a lattice anchored at `base` with spacing `pitch`.
fn index_span(low: f32, high: f32, base: f32, pitch: f32) -> (i32, i32) {
    let first = libm::ceilf((low - base) / pitch - 0.5);
    let last = libm::floorf((high - base) / pitch - 0.5);
    // The inputs are finite and `pitch > 0`, so these are finite; the casts
    // saturate rather than wrap, and an empty span is caught by the caller.
    (first as i32, last as i32)
}

/// Is a lattice cell center populated? Inside the outline, and no closer than
/// `epsilon` to any of its segments — see [`filled_polygon_cells`] step 3.
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
/// first). Points exactly on an edge are undefined here on purpose — the
/// ε-inset in [`cell_is_populated`] rejects them before the answer matters.
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

/// One physical strand with its full patch-path address: which object (by
/// stable id, when the object has one), which repeat-instance chain, and
/// the fixture-relative lamp range it occupies.
///
/// This is the table [`crate::resolve_patch`] lowers [`crate::MapObjectPath`]
/// entries through — the bridge between "/sector/2" and "lamps 60..90".
/// Strand order matches [`ResolvedMap2d::spans`] exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInstanceSpan {
    /// The owning object's index in `doc.objects` — always present: the
    /// resolver knows every strand's object whether or not it has an id,
    /// so DISPLAY can expand id-less documents too (grain robustness:
    /// old-format data must never collapse the effective tree).
    pub object: usize,
    /// The owning object's stable id; `None` when the document has not been
    /// through ensure-ids (such strands are unaddressable by path — patch
    /// ENTRIES need the id, display does not).
    pub id: Option<crate::map2d_object_id::Map2dObjectId>,
    /// Repeat-instance indices, outermost first; empty for a plain shape.
    pub instances: Vec<u32>,
    /// Zero-based wiring index of the strand's first lamp.
    pub start: u32,
    pub count: u32,
}

/// The per-strand instance-address table for a resolved document.
///
/// Strand k of an object corresponds to instance path k in the shape's
/// instance enumeration — the same order [`resolve`] emits spans in (a
/// repeat resolves instance 0's strands, then instance 1's, …), pinned by
/// test against the resolver.
#[must_use]
pub fn object_instance_spans(doc: &Map2dDoc, resolved: &ResolvedMap2d) -> Vec<ObjectInstanceSpan> {
    let mut per_object_paths: Vec<Vec<Vec<u32>>> = Vec::with_capacity(doc.objects.len());
    for object in &doc.objects {
        per_object_paths.push(shape_instance_paths(&object.shape));
    }
    let mut cursor = alloc::vec![0usize; doc.objects.len()];
    let mut spans = Vec::with_capacity(resolved.spans.len());
    for span in &resolved.spans {
        let object = span.object as usize;
        let instances = per_object_paths
            .get(object)
            .and_then(|paths| paths.get(cursor[object]))
            .cloned()
            .unwrap_or_default();
        cursor[object] += 1;
        spans.push(ObjectInstanceSpan {
            object,
            id: doc.objects.get(object).and_then(|object| object.id.clone()),
            instances,
            start: span.start,
            count: span.count,
        });
    }
    spans
}

/// Every strand's instance path for one shape, in wiring order: a leaf is
/// one strand with no steps; a repeat prefixes each inner path with its
/// instance index.
fn shape_instance_paths(shape: &Map2dShape) -> Vec<Vec<u32>> {
    match shape {
        Map2dShape::Repeat(repeat) => {
            let inner = shape_instance_paths(&repeat.shape);
            let mut paths = Vec::with_capacity(inner.len() * repeat.count.max(1) as usize);
            for instance in 0..repeat.count.max(1) {
                for inner_path in &inner {
                    let mut path = Vec::with_capacity(1 + inner_path.len());
                    path.push(instance);
                    path.extend_from_slice(inner_path);
                    paths.push(path);
                }
            }
            paths
        }
        _ => alloc::vec![Vec::new()],
    }
}

/// The rotation-stride hint an object's UI steps `offset` by, in lamps.
///
/// An explicit [`Map2dObject::stride`] override wins; otherwise the stride
/// derives from the shape kind — see [`shape_stride`]. Never zero.
#[must_use]
pub fn object_stride(object: &Map2dObject) -> u32 {
    object
        .stride
        .filter(|stride| *stride > 0)
        .unwrap_or_else(|| shape_stride(&object.shape))
}

/// The derived rotation stride of one shape kind, in lamps. Never zero.
///
/// - Grid: one row (`cols`).
/// - Ring: the **outer** count — inner rings derive their own counts
///   ([`derived_ring_count`]), so no single number is honest for every
///   ring; the outer count is the closest thing and override territory
///   beyond it.
/// - Path: 1 (no intrinsic period).
/// - Polygon: lamps per side (`count / points.len()`) when the perimeter
///   divides evenly, else 1 (override territory) — a triangular door of 9
///   lamps rotates by 3, one side at a time.
/// - FilledPolygon: 1. A shaped matrix's rows are exactly what the outline
///   makes them — a triangle's rows grow one cell at a time — so no single
///   number is an honest period, the way `cols` is for a grid. Override
///   territory, like the non-divisible polygon.
/// - Repeat: the inner shape's whole lamp count — rotating a repeat-object
///   entry by its stride steps one instance.
#[must_use]
pub fn shape_stride(shape: &Map2dShape) -> u32 {
    match shape {
        Map2dShape::Grid(grid) => grid.cols.max(1),
        Map2dShape::Ring(ring) => ring.outer_count.max(1),
        Map2dShape::Path(_) => 1,
        Map2dShape::Polygon(polygon) => {
            let sides = polygon.points.len() as u32;
            if sides > 0 && polygon.count % sides == 0 {
                (polygon.count / sides).max(1)
            } else {
                1
            }
        }
        Map2dShape::FilledPolygon(_) => 1,
        Map2dShape::Repeat(repeat) => shape_lamp_count(&repeat.shape).max(1),
    }
}

/// The lamp count one shape resolves to.
///
/// Mirrors [`resolve`] exactly: most kinds are pure parameter arithmetic, the
/// ring reuses the same [`derived_ring_count`] derivation, and the filled
/// polygon runs the resolver's own lattice walk ([`filled_polygon_cells`]) —
/// its count is derived, so there is no arithmetic that could mirror it. A
/// test pins all of them together over every shape kind, so a resolver change
/// that moved a count would fail loudly rather than skew stride math.
#[must_use]
pub fn shape_lamp_count(shape: &Map2dShape) -> u32 {
    match shape {
        Map2dShape::Grid(grid) => grid.cols * grid.rows,
        Map2dShape::Ring(ring) => {
            let mut total = 0;
            for ring_index in 0..ring.rings {
                let radius = ring.radius * (ring.rings - ring_index) as f32 / ring.rings as f32;
                total += ring
                    .counts
                    .get(ring_index as usize)
                    .copied()
                    .filter(|count| *count > 0)
                    .unwrap_or_else(|| derived_ring_count(ring.outer_count, radius, ring.radius));
            }
            total
        }
        Map2dShape::Path(path) => path.count,
        Map2dShape::Polygon(polygon) => polygon.count,
        Map2dShape::FilledPolygon(filled) => filled_polygon_cells(filled).len() as u32,
        Map2dShape::Repeat(repeat) => repeat.count * shape_lamp_count(&repeat.shape),
    }
}

/// Per-segment inert flags, already mapped into walk order.
///
/// `reversed` mirrors the segment order along with the points
/// (`n_segments - 1 - i`), so the same *physical* segments stay inert whichever
/// end of the strip the data enters from. Out-of-range indices name no segment
/// and are ignored — the editor sanitizes them out, and a hand-authored
/// document should not fail to resolve over one.
fn inert_segments(path: &PathShape) -> Vec<bool> {
    let segments = path.points.len().saturating_sub(1);
    let mut inert = Vec::new();
    inert.resize(segments, false);
    for gap in &path.gaps {
        let index = *gap as usize;
        if index < segments {
            let index = if path.reversed {
                segments - 1 - index
            } else {
                index
            };
            inert[index] = true;
        }
    }
    inert
}

/// Summed length of the segments that carry lamps.
fn active_length(points: &[[f32; 2]], inert: &[bool]) -> f32 {
    points
        .windows(2)
        .enumerate()
        .filter(|(index, _)| !is_inert(inert, *index))
        .map(|(_, pair)| distance(pair[0], pair[1]))
        .sum()
}

/// The point `target_distance` along the polyline's **active** length.
///
/// Inert segments move the walk's position without consuming distance, so a
/// lamp can never land on one. The fallback (float slop pushing the last lamp
/// a hair past the total) is the far end of the last *active* segment, which
/// is not `points.last()` when the path ends in a gap. Degenerate zero-length
/// segments keep their long-standing skip behavior.
fn point_at_active_distance(points: &[[f32; 2]], inert: &[bool], target_distance: f32) -> [f32; 2] {
    let mut remaining = target_distance;
    let mut last_active_end = None;
    for (index, pair) in points.windows(2).enumerate() {
        if is_inert(inert, index) {
            continue;
        }
        let start = pair[0];
        let end = pair[1];
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
        last_active_end = Some(end);
    }
    last_active_end.unwrap_or_else(|| *points.first().expect("non-empty points"))
}

fn is_inert(inert: &[bool], index: usize) -> bool {
    inert.get(index).copied().unwrap_or(false)
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    libm::sqrtf(dx * dx + dy * dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map2d_doc::{Map2dObject, Map2dShape, PathAlign};
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;

    #[test]
    fn snake_grid_alternates_row_direction() {
        let resolved = resolve_shape(Map2dShape::Grid(GridShape {
            origin: [0.0, 0.0],
            cols: 3,
            rows: 2,
            pitch: 10.0,
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        }));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(
            positions,
            vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [20.0, 0.0],
                [20.0, 10.0],
                [10.0, 10.0],
                [0.0, 10.0],
            ]
        );
    }

    #[test]
    fn raster_grid_repeats_row_direction() {
        let resolved = resolve_shape(Map2dShape::Grid(GridShape {
            origin: [0.0, 0.0],
            cols: 2,
            rows: 2,
            pitch: 10.0,
            routing: GridRouting::Raster,
            start_corner: GridCorner::Tl,
        }));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(
            positions,
            vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]]
        );
    }

    #[test]
    fn bottom_right_snake_grid_starts_at_that_corner() {
        let resolved = resolve_shape(Map2dShape::Grid(GridShape {
            origin: [0.0, 0.0],
            cols: 2,
            rows: 2,
            pitch: 10.0,
            routing: GridRouting::Snake,
            start_corner: GridCorner::Br,
        }));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(
            positions,
            vec![[10.0, 10.0], [0.0, 10.0], [0.0, 0.0], [10.0, 0.0]]
        );
    }

    #[test]
    fn ring_starts_at_twelve_oclock_and_runs_clockwise() {
        let resolved = resolve_shape(Map2dShape::Ring(RingShape {
            center: [0.0, 0.0],
            radius: 10.0,
            outer_count: 4,
            rings: 1,
            counts: Vec::new(),
            order: RingOrder::OuterFirst,
            start_angle_deg: -90.0,
            dir: RingDir::Cw,
        }));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        // -90° is up (y-down screen space); cw then heads to +x.
        assert!(positions[0][1] < -9.9);
        assert!(positions[1][0] > 9.9);
        assert!(positions[2][1] > 9.9);
        assert!(positions[3][0] < -9.9);
    }

    #[test]
    fn multi_ring_derives_inner_counts_and_honors_order() {
        let outer_first = resolve_shape(Map2dShape::Ring(button_rings()));
        assert_eq!(outer_first.lamps.len(), 24); // 16 outer + 8 inner
        let outer_radius_first = radius_of(outer_first.lamps[0].pos);
        assert!((outer_radius_first - 90.0).abs() < 0.01);

        let mut inner_first_shape = button_rings();
        inner_first_shape.order = RingOrder::InnerFirst;
        let inner_first = resolve_shape(Map2dShape::Ring(inner_first_shape));
        assert_eq!(inner_first.lamps.len(), 24);
        let inner_radius_first = radius_of(inner_first.lamps[0].pos);
        assert!((inner_radius_first - 45.0).abs() < 0.01);
    }

    #[test]
    fn path_lamps_sit_exactly_on_endpoints() {
        let resolved = resolve_shape(Map2dShape::Path(PathShape {
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            count: 5,
            reversed: false,
            gaps: Vec::new(),
            align: PathAlign::On,
        }));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(positions[0], [0.0, 0.0]);
        assert_eq!(positions[4], [10.0, 10.0]);
        assert_eq!(positions[2], [10.0, 0.0]); // halfway along the 20-unit run
    }

    #[test]
    fn reversed_path_swaps_endpoints() {
        let resolved = resolve_shape(Map2dShape::Path(PathShape {
            points: vec![[0.0, 0.0], [10.0, 0.0]],
            count: 2,
            reversed: true,
            gaps: Vec::new(),
            align: PathAlign::On,
        }));
        assert_eq!(resolved.lamps[0].pos, [10.0, 0.0]);
        assert_eq!(resolved.lamps[1].pos, [0.0, 0.0]);
    }

    /// The heart of inert segments: `count` lamps spread over the ACTIVE
    /// length only, at one uniform pitch, and never onto the jumper. The
    /// lamp landing exactly on the seam sits at the *end* of the active run
    /// before the gap (the walk's `remaining <= segment` convention).
    #[test]
    fn a_mid_path_gap_carries_no_lamps_and_keeps_the_pitch_uniform() {
        let resolved = resolve_shape(Map2dShape::Path(gapped_l(vec![1], 5, false)));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        // 20 active units (segments 0 and 2), 5 lamps → 5 units apart.
        assert_eq!(
            positions,
            vec![
                [0.0, 0.0],
                [5.0, 0.0],
                [10.0, 0.0],  // end of the run into the jumper
                [15.0, 10.0], // resumes on the far side, pitch unbroken
                [20.0, 10.0],
            ]
        );
        // Inert segments emit NO entries: the object's span is exactly `count`,
        // so every downstream wiring index is unshifted.
        assert_eq!(resolved.spans[0].count, 5);
    }

    #[test]
    fn a_leading_gap_starts_the_lamps_at_the_first_active_segment() {
        let resolved = resolve_shape(Map2dShape::Path(gapped_l(vec![0], 3, false)));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(positions, vec![[10.0, 0.0], [10.0, 10.0], [20.0, 10.0]]);
    }

    #[test]
    fn a_trailing_gap_ends_the_lamps_at_the_last_active_segment() {
        let resolved = resolve_shape(Map2dShape::Path(gapped_l(vec![2], 3, false)));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(positions, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
    }

    #[test]
    fn gaps_either_side_leave_one_active_segment_carrying_every_lamp() {
        let resolved = resolve_shape(Map2dShape::Path(gapped_l(vec![0, 2], 3, false)));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(positions, vec![[10.0, 0.0], [10.0, 5.0], [10.0, 10.0]]);
    }

    /// `reversed` mirrors the gap indices along with the points, so the same
    /// *physical* segment stays inert whichever end the data enters from — the
    /// reversed lamp list is exactly the forward one, backwards.
    #[test]
    fn reversed_remaps_gap_indices_to_the_same_physical_segments() {
        let forward = resolve_shape(Map2dShape::Path(gapped_l(vec![0], 3, false)));
        let reversed = resolve_shape(Map2dShape::Path(gapped_l(vec![0], 3, true)));
        let mut expected: Vec<_> = forward.lamps.iter().map(|l| l.pos).collect();
        expected.reverse();
        let actual: Vec<_> = reversed.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(actual, expected);
        assert_eq!(actual[0], [20.0, 10.0]);
    }

    #[test]
    fn a_single_lamp_on_a_gapped_path_sits_at_the_first_active_point() {
        let resolved = resolve_shape(Map2dShape::Path(gapped_l(vec![0], 1, false)));
        assert_eq!(resolved.lamps.len(), 1);
        assert_eq!(resolved.lamps[0].pos, [10.0, 0.0]);
    }

    /// Every segment inert is the analogue of a zero-length path: an object
    /// that cannot place a lamp is a load-time error, not silence.
    #[test]
    fn a_path_whose_every_segment_is_a_gap_is_invalid() {
        let doc = Map2dDoc {
            objects: vec![object(Map2dShape::Path(gapped_l(vec![0, 1, 2], 4, false)))],
            ..Map2dDoc::new()
        };
        let error = resolve(&doc).unwrap_err();
        assert!(matches!(
            &error,
            Map2dError::InvalidObject { reason, .. } if reason.contains("no active length")
        ));
    }

    /// The jumper's lamp-index seam, as a renderer drawing the strand as a
    /// BODY needs it: the lamps split exactly where the resolver's positions
    /// jump the gap, and a jumper with every lamp on one side of it splits
    /// nothing.
    #[test]
    fn path_gap_breaks_name_the_lamp_the_jumper_resumes_at() {
        let path = gapped_l(vec![1], 5, false);
        assert_eq!(path_gap_breaks(&path), vec![3]);
        // The break is where the positions themselves jump: lamps 0..3 walk
        // the first run, 3..5 the far side.
        let positions = resolve_shape(Map2dShape::Path(path)).positions();
        assert_eq!(positions[2], [10.0, 0.0]);
        assert_eq!(positions[3], [15.0, 10.0]);

        // Entered from the other end the same physical segment is the
        // jumper, and the seam lamp still ends the run it is walking INTO
        // the gap — so the break lands at the same offset, counted along
        // the reversed travel.
        assert_eq!(path_gap_breaks(&gapped_l(vec![1], 5, true)), vec![3]);
        // Leading and trailing jumpers carry no lamps past them.
        assert!(path_gap_breaks(&gapped_l(vec![0], 3, false)).is_empty());
        assert!(path_gap_breaks(&gapped_l(vec![2], 3, false)).is_empty());
        // A gapless path never breaks, nor does a one-lamp strand.
        assert!(path_gap_breaks(&gapped_l(Vec::new(), 5, false)).is_empty());
        assert!(path_gap_breaks(&gapped_l(vec![1], 1, false)).is_empty());
    }

    /// Out-of-range gap indices name no segment; the document still resolves
    /// (the editor sanitizes them away, a hand-edit should not brick a load).
    #[test]
    fn out_of_range_gap_indices_are_ignored() {
        let with_junk = resolve_shape(Map2dShape::Path(gapped_l(vec![1, 9, 42], 5, false)));
        let clean = resolve_shape(Map2dShape::Path(gapped_l(vec![1], 5, false)));
        assert_eq!(with_junk.positions(), clean.positions());
    }

    // ---- polygon ---------------------------------------------------------

    /// The polygon closes implicitly and wraps instead of doubling an
    /// endpoint: 9 lamps over an equilateral-perimeter triangle land 3 per
    /// side with lamps 0/3/6 exactly on the corners — the per-side
    /// regularity the intrinsic stride reads off.
    #[test]
    fn polygon_distributes_lamps_around_the_closed_perimeter() {
        // A 3-4-5-ish right triangle scaled so all sides sum to 36:
        // use an equilateral-by-length layout instead — three 12-unit sides.
        let resolved = resolve_shape(Map2dShape::Polygon(triangle_12()));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        assert_eq!(positions.len(), 9);
        // Lamp 0 on vertex 0; lamps 3 and 6 on the other corners.
        assert_eq!(positions[0], [0.0, 0.0]);
        assert!(close(positions[3], [12.0, 0.0]), "{:?}", positions[3]);
        assert!(close(positions[6], [6.0, 10.392305]), "{:?}", positions[6]);
        // Even 4-unit spacing along the first side, no doubled endpoint.
        assert!(close(positions[1], [4.0, 0.0]));
        assert!(close(positions[2], [8.0, 0.0]));
        // The last lamp sits one step short of wrapping back onto vertex 0.
        assert!(positions[8] != positions[0]);
        // One strand, exactly `count` lamps.
        assert_eq!(resolved.spans.len(), 1);
        assert_eq!(resolved.spans[0].count, 9);
    }

    #[test]
    fn degenerate_polygons_are_invalid() {
        for (shape, needle) in [
            (
                Map2dShape::Polygon(PolygonShape {
                    points: vec![[0.0, 0.0], [1.0, 0.0]],
                    count: 3,
                    align: PathAlign::On,
                }),
                "at least 3 points",
            ),
            (
                Map2dShape::Polygon(PolygonShape {
                    points: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
                    count: 0,
                    align: PathAlign::On,
                }),
                "count",
            ),
            (
                Map2dShape::Polygon(PolygonShape {
                    points: vec![[2.0, 2.0], [2.0, 2.0], [2.0, 2.0]],
                    count: 3,
                    align: PathAlign::On,
                }),
                "zero perimeter",
            ),
        ] {
            let doc = Map2dDoc {
                objects: vec![object(shape)],
                ..Map2dDoc::new()
            };
            let error = resolve(&doc).unwrap_err();
            assert!(
                matches!(&error, Map2dError::InvalidObject { reason, .. } if reason.contains(needle)),
                "wanted {needle:?} in {error:?}"
            );
        }
    }

    // ---- filled polygon --------------------------------------------------

    /// The lattice walk, hand-computed. A right triangle `(0,0) (40,0)
    /// (0,40)` over a 10-unit lattice: cell centers land on the odd fives,
    /// the hypotenuse `x + y = 40` passes exactly through `(35,5)`, `(25,15)`,
    /// `(15,25)` and `(5,35)` — every one of them ε-excluded — and the top
    /// row is left completely empty. Six cells, wired as a snake from the
    /// top-left.
    #[test]
    fn a_filled_polygon_snakes_its_populated_cells_row_by_row() {
        let resolved = resolve_shape(Map2dShape::FilledPolygon(right_triangle_40()));
        assert_eq!(
            resolved.positions(),
            vec![
                [5.0, 5.0],
                [15.0, 5.0],
                [25.0, 5.0], // row 0, left to right
                [15.0, 15.0],
                [5.0, 15.0], // row 1, snaked back
                [5.0, 25.0], // row 2, forward again
            ]
        );
        // One strand, and the count is exactly what got placed — no authored
        // number to disagree with.
        assert_eq!(resolved.spans.len(), 1);
        assert_eq!(resolved.spans[0].count, 6);
    }

    /// The same outline under raster routing and each start corner — the walk
    /// mirrors `resolve_grid`'s flips, applied to populated cells only.
    #[test]
    fn filled_polygon_routing_and_start_corner_mirror_the_grid_walk() {
        let raster = FilledPolygonShape {
            routing: GridRouting::Raster,
            ..right_triangle_40()
        };
        assert_eq!(
            resolve_shape(Map2dShape::FilledPolygon(raster)).positions(),
            vec![
                [5.0, 5.0],
                [15.0, 5.0],
                [25.0, 5.0],
                [5.0, 15.0],
                [15.0, 15.0],
                [5.0, 25.0],
            ]
        );

        // Bottom-right: rows walked bottom-up, first row right-to-left.
        let bottom_right = FilledPolygonShape {
            start_corner: GridCorner::Br,
            ..right_triangle_40()
        };
        assert_eq!(
            resolve_shape(Map2dShape::FilledPolygon(bottom_right)).positions(),
            vec![
                [5.0, 25.0],
                [5.0, 15.0],
                [15.0, 15.0],
                [25.0, 5.0],
                [15.0, 5.0],
                [5.0, 5.0],
            ]
        );

        // Top-right: rows top-down, but the first one entered from the right.
        let top_right = FilledPolygonShape {
            start_corner: GridCorner::Tr,
            ..right_triangle_40()
        };
        assert_eq!(
            resolve_shape(Map2dShape::FilledPolygon(top_right)).positions(),
            vec![
                [25.0, 5.0],
                [15.0, 5.0],
                [5.0, 5.0],
                [5.0, 15.0],
                [15.0, 15.0],
                [5.0, 25.0],
            ]
        );
    }

    /// The ε-inset is the determinism tie-break: a cell center sitting exactly
    /// on an edge is excluded, always — the count never turns on which way a
    /// float rounded. And it damps: nudging that edge by ±ε/2 moves no cell
    /// across the line, so a vertex drag does not make the lattice blink.
    #[test]
    fn the_epsilon_inset_excludes_centers_on_an_edge_and_damps_a_nudge() {
        // A 40×25 rectangle on a 10-unit lattice: rows at y = 5, 15, 25 — and
        // the bottom edge runs exactly through y = 25.
        let interior = vec![
            [5.0, 5.0],
            [15.0, 5.0],
            [25.0, 5.0],
            [35.0, 5.0],
            [35.0, 15.0],
            [25.0, 15.0],
            [15.0, 15.0],
            [5.0, 15.0],
        ];
        let on_the_edge = rectangle(40.0, 25.0);
        assert_eq!(
            resolve_shape(Map2dShape::FilledPolygon(on_the_edge)).positions(),
            interior,
            "centers on the bottom edge must be excluded"
        );

        // ε = pitch × 1e-3 = 0.01. Half of that, either way, changes nothing:
        // the edge row stays out and the interior cells do not budge.
        for nudge in [0.005f32, -0.005] {
            let nudged = resolve_shape(Map2dShape::FilledPolygon(rectangle(40.0, 25.0 + nudge)));
            let positions = nudged.positions();
            assert_eq!(positions.len(), interior.len(), "nudge {nudge}");
            for (actual, want) in positions.iter().zip(&interior) {
                assert!(
                    close(*actual, *want),
                    "nudge {nudge}: {actual:?} != {want:?}"
                );
            }
        }

        // A frank move past the band DOES populate the row — the inset is a
        // hair, not a hidden margin.
        let clear = resolve_shape(Map2dShape::FilledPolygon(rectangle(40.0, 26.0)));
        assert_eq!(clear.lamps.len(), 12);
    }

    /// `angle_deg` turns the LATTICE, not the outline: the same square packs
    /// differently at 45°, and the doc-space cells are no longer axis-aligned.
    #[test]
    fn the_lattice_angle_repacks_the_same_outline() {
        let square = FilledPolygonShape {
            points: vec![[0.0, 0.0], [30.0, 0.0], [30.0, 30.0], [0.0, 30.0]],
            pitch: 10.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        };
        let upright = resolve_shape(Map2dShape::FilledPolygon(square.clone()));
        assert_eq!(upright.lamps.len(), 9);
        // Zero degrees is the plain axis-aligned walk, exactly — an author
        // who has not turned the lattice sees round numbers.
        assert_eq!(
            upright.positions(),
            vec![
                [5.0, 5.0],
                [15.0, 5.0],
                [25.0, 5.0],
                [25.0, 15.0],
                [15.0, 15.0],
                [5.0, 15.0],
                [5.0, 25.0],
                [15.0, 25.0],
                [25.0, 25.0],
            ]
        );

        let turned = resolve_shape(Map2dShape::FilledPolygon(FilledPolygonShape {
            angle_deg: 45.0,
            ..square
        }));
        // A diagonal lattice fits ten cells in the same square, and none of
        // them sits on the upright lattice's coordinates.
        assert_eq!(turned.lamps.len(), 10);
        for lamp in &turned.lamps {
            assert!(
                !upright
                    .lamps
                    .iter()
                    .any(|upright| close(upright.pos, lamp.pos)),
                "turned lattice landed on an upright cell: {:?}",
                lamp.pos
            );
        }
    }

    /// `origin` slides the lattice phase in the lattice frame — a half-pitch
    /// nudge moves every cell by exactly that, and can change which cells the
    /// outline admits at all.
    #[test]
    fn the_origin_nudge_shifts_the_lattice_phase() {
        let flush = resolve_shape(Map2dShape::FilledPolygon(rectangle(40.0, 20.0)));
        assert_eq!(flush.lamps.len(), 8);

        let nudged = resolve_shape(Map2dShape::FilledPolygon(FilledPolygonShape {
            origin: [5.0, 0.0],
            ..rectangle(40.0, 20.0)
        }));
        // Half a pitch to the right: the right-hand column would land on the
        // edge, so the row loses a cell instead of gaining one.
        assert_eq!(nudged.lamps.len(), 6);
        assert_eq!(
            nudged.positions(),
            vec![
                [10.0, 5.0],
                [20.0, 5.0],
                [30.0, 5.0],
                [30.0, 15.0],
                [20.0, 15.0],
                [10.0, 15.0],
            ]
        );
    }

    /// Snake parity counts VISITED rows: a row the outline skips entirely is
    /// not a row the chain reaches, so it must not flip the next row's
    /// direction. An hourglass with a pinched-out middle row is the case.
    #[test]
    fn an_unpopulated_row_does_not_flip_the_snake_direction() {
        // Two 30-wide lobes joined by a waist spanning x ∈ (16, 18) — too
        // narrow to hold any cell center, which sit on the odd fives. Rows at
        // y = 5 and y = 45 populate; the row at y = 25 does not.
        let hourglass = FilledPolygonShape {
            points: vec![
                [0.0, 0.0],
                [30.0, 0.0],
                [18.0, 24.0],
                [18.0, 26.0],
                [30.0, 50.0],
                [0.0, 50.0],
                [16.0, 26.0],
                [16.0, 24.0],
            ],
            pitch: 10.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        };
        let positions = resolve_shape(Map2dShape::FilledPolygon(hourglass)).positions();
        let rows: Vec<f32> = positions.iter().map(|pos| pos[1]).collect();
        assert!(
            !rows.contains(&25.0),
            "the waist row must be empty: {positions:?}"
        );
        // Row y=5 runs left→right; the NEXT visited row (y=45) runs
        // right→left. If the empty waist row had consumed a parity step, this
        // row would run left→right too.
        let top: Vec<[f32; 2]> = positions.iter().copied().filter(|p| p[1] == 5.0).collect();
        let bottom: Vec<[f32; 2]> = positions.iter().copied().filter(|p| p[1] == 45.0).collect();
        assert!(top.len() > 1 && bottom.len() > 1, "{positions:?}");
        assert!(top[0][0] < top[top.len() - 1][0], "top row runs forward");
        assert!(
            bottom[0][0] > bottom[bottom.len() - 1][0],
            "bottom row must run backward: {bottom:?}"
        );
    }

    /// The derived count is deterministic: two resolves of one document are
    /// the same lamps, and `filled_polygon_cells` is the only walk either the
    /// resolver or `shape_lamp_count` runs.
    #[test]
    fn a_filled_polygon_resolves_deterministically() {
        let doc = Map2dDoc {
            objects: vec![object(Map2dShape::FilledPolygon(right_triangle_40()))],
            ..Map2dDoc::new()
        };
        assert_eq!(resolve(&doc).unwrap(), resolve(&doc).unwrap());
        assert_eq!(
            filled_polygon_cells(&right_triangle_40()),
            filled_polygon_cells(&right_triangle_40())
        );
    }

    /// The degenerate cases, each naming what is wrong with the object.
    #[test]
    fn degenerate_filled_polygons_are_invalid() {
        for (shape, needle) in [
            (
                FilledPolygonShape {
                    points: vec![[0.0, 0.0], [10.0, 0.0]],
                    ..right_triangle_40()
                },
                "at least 3 points",
            ),
            (
                FilledPolygonShape {
                    pitch: 0.0,
                    ..right_triangle_40()
                },
                "pitch must be positive",
            ),
            (
                FilledPolygonShape {
                    pitch: -4.0,
                    ..right_triangle_40()
                },
                "pitch must be positive",
            ),
            (
                FilledPolygonShape {
                    pitch: f32::NAN,
                    ..right_triangle_40()
                },
                "pitch must be positive",
            ),
            (
                FilledPolygonShape {
                    points: vec![[2.0, 2.0], [2.0, 2.0], [2.0, 2.0]],
                    ..right_triangle_40()
                },
                "no lattice cells",
            ),
            (
                // A pitch coarser than the outline places nothing.
                FilledPolygonShape {
                    pitch: 500.0,
                    ..right_triangle_40()
                },
                "no lattice cells",
            ),
            (
                FilledPolygonShape {
                    points: vec![[0.0, 0.0], [f32::NAN, 0.0], [0.0, 40.0]],
                    ..right_triangle_40()
                },
                "non-finite",
            ),
        ] {
            let doc = Map2dDoc {
                objects: vec![object(Map2dShape::FilledPolygon(shape))],
                ..Map2dDoc::new()
            };
            let error = resolve(&doc).unwrap_err();
            assert!(
                matches!(&error, Map2dError::InvalidObject { reason, .. } if reason.contains(needle)),
                "wanted {needle:?} in {error:?}"
            );
        }
    }

    /// The right triangle every filled-polygon walk test reads off: legs on
    /// the axes, 40 units each, on a 10-unit lattice.
    fn right_triangle_40() -> FilledPolygonShape {
        FilledPolygonShape {
            points: vec![[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]],
            pitch: 10.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        }
    }

    /// An axis-aligned rectangle anchored at the origin, on a 10-unit lattice.
    fn rectangle(width: f32, height: f32) -> FilledPolygonShape {
        FilledPolygonShape {
            points: vec![[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]],
            pitch: 10.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        }
    }

    // ---- stride hints ----------------------------------------------------

    /// The derived stride per shape kind (D41): the number the UI steps a
    /// rotation offset by.
    #[test]
    fn shape_stride_derives_per_kind() {
        // Grid: one row.
        assert_eq!(
            shape_stride(&Map2dShape::Grid(GridShape {
                origin: [0.0, 0.0],
                cols: 16,
                rows: 4,
                pitch: 10.0,
                routing: GridRouting::Snake,
                start_corner: GridCorner::Tl,
            })),
            16
        );
        // Ring: the outer count (inner counts derive per ring — the outer
        // number is the honest single stride).
        assert_eq!(shape_stride(&Map2dShape::Ring(button_rings())), 16);
        // Path: no intrinsic period.
        assert_eq!(shape_stride(&Map2dShape::Path(square_corner_path())), 1);
        // Polygon, evenly divisible: lamps per side (the radiance door).
        assert_eq!(shape_stride(&Map2dShape::Polygon(triangle_12())), 3);
        // Polygon, not divisible: 1 — override territory.
        assert_eq!(
            shape_stride(&Map2dShape::Polygon(PolygonShape {
                points: vec![[0.0, 0.0], [12.0, 0.0], [6.0, 10.392305]],
                count: 10,
                align: PathAlign::On,
            })),
            1
        );
        // Filled polygon: 1 — the outline decides each row's length, so
        // there is no honest period. Override territory.
        assert_eq!(
            shape_stride(&Map2dShape::FilledPolygon(right_triangle_40())),
            1
        );
        // Repeat: the inner shape's whole lamp count — one instance per step.
        assert_eq!(shape_stride(&repeated_sector(5)), 4);
    }

    /// An explicit object-level `stride` beats the derivation; zero is
    /// nonsense and falls back.
    #[test]
    fn object_stride_prefers_the_authored_override() {
        let mut object = object(Map2dShape::Polygon(triangle_12()));
        assert_eq!(object_stride(&object), 3);
        object.stride = Some(5);
        assert_eq!(object_stride(&object), 5);
        object.stride = Some(0);
        assert_eq!(object_stride(&object), 3);
    }

    /// `shape_lamp_count` mirrors the resolver by construction; this pin
    /// makes the mirror a tested contract over every shape kind, nesting
    /// and derived ring counts included.
    #[test]
    fn shape_lamp_count_matches_the_resolver_for_every_kind() {
        let shapes = [
            Map2dShape::Grid(GridShape {
                origin: [0.0, 0.0],
                cols: 7,
                rows: 3,
                pitch: 5.0,
                routing: GridRouting::Snake,
                start_corner: GridCorner::Tl,
            }),
            Map2dShape::Ring(button_rings()),
            Map2dShape::Path(square_corner_path()),
            Map2dShape::Polygon(triangle_12()),
            // The derived-count case — the whole reason this pin matters
            // now: nothing else cross-checks a filled polygon's count.
            Map2dShape::FilledPolygon(right_triangle_40()),
            Map2dShape::FilledPolygon(FilledPolygonShape {
                angle_deg: 45.0,
                start_corner: GridCorner::Br,
                ..right_triangle_40()
            }),
            Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::FilledPolygon(right_triangle_40())),
                center: [0.0, 0.0],
                count: 3,
            }),
            repeated_sector(5),
            Map2dShape::Repeat(RepeatShape {
                shape: Box::new(repeated_sector(2)),
                center: [100.0, 100.0],
                count: 3,
            }),
        ];
        for shape in shapes {
            let expected = resolve_shape(shape.clone()).lamps.len() as u32;
            assert_eq!(shape_lamp_count(&shape), expected, "{shape:?}");
        }
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    /// An equilateral triangle with 12-unit sides (36-unit perimeter) and 9
    /// lamps — the triangular-panel archetype: 3 lamps per side, stride 3.
    fn triangle_12() -> PolygonShape {
        PolygonShape {
            points: vec![[0.0, 0.0], [12.0, 0.0], [6.0, 10.392305]],
            count: 9,
            align: PathAlign::On,
        }
    }

    // ---- rotational repeat ----------------------------------------------

    /// The rotation itself, read off a shape whose quarter turns are exact
    /// corners: a two-lamp diagonal repeated four times traces a square, each
    /// instance a quarter turn on from the last (screen coordinates, y-down,
    /// so a positive turn runs clockwise).
    #[test]
    fn repeat_turns_each_instance_by_its_share_of_the_circle() {
        let resolved = resolve_shape(Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(PathShape {
                points: vec![[10.0, 0.0], [0.0, 10.0]],
                count: 2,
                reversed: false,
                gaps: Vec::new(),
                align: PathAlign::On,
            })),
            center: [0.0, 0.0],
            count: 4,
        }));
        let positions: Vec<_> = resolved.lamps.iter().map(|l| l.pos).collect();
        let expected = [
            [10.0, 0.0],
            [0.0, 10.0], // instance 0: unrotated
            [0.0, 10.0],
            [-10.0, 0.0], // 90°
            [-10.0, 0.0],
            [0.0, -10.0], // 180°
            [0.0, -10.0],
            [10.0, 0.0], // 270°
        ];
        assert_eq!(positions.len(), expected.len());
        for (index, (actual, want)) in positions.iter().zip(expected).enumerate() {
            assert!(
                (actual[0] - want[0]).abs() < 1e-3 && (actual[1] - want[1]).abs() < 1e-3,
                "lamp {index}: {actual:?} != {want:?}"
            );
        }
    }

    /// Instance 0 is the inner shape *untouched*, not merely close to it —
    /// `sinf(0)`/`cosf(0)` are exact, and an author who repeats a traced
    /// sector must not see the traced copy move.
    #[test]
    fn repeat_leaves_the_first_instance_bit_identical() {
        let inner = square_corner_path();
        let plain = resolve_shape(Map2dShape::Path(inner.clone()));
        let repeated = resolve_shape(Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(inner)),
            center: [17.0, -4.0],
            count: 7,
        }));
        assert_eq!(
            repeated.positions()[..plain.lamps.len()].to_vec(),
            plain.positions()
        );
    }

    /// Wiring order is instance by instance: all of instance 0's lamps, then
    /// all of instance 1's. Instance `k` is physical strand `k`.
    #[test]
    fn repeat_wires_each_instance_through_before_the_next() {
        let resolved = resolve_shape(repeated_sector(3));
        assert_eq!(resolved.lamps.len(), 12);
        // Each instance's four lamps share a turn; a strand that interleaved
        // instances would break the run of equal angular offsets.
        for instance in 0..3usize {
            for lamp in 0..4usize {
                let base = resolved.lamps[lamp].pos;
                let here = resolved.lamps[instance * 4 + lamp].pos;
                let turn = angle_of(here) - angle_of(base);
                let expected = instance as f32 * 120.0;
                assert!(
                    (wrap_degrees(turn - expected)).abs() < 0.05,
                    "instance {instance} lamp {lamp}: turned {turn}°, wanted {expected}°"
                );
            }
        }
    }

    /// The load-bearing decision: **one span per instance**, all naming the
    /// same document object. Downstream these spans are the physical strands —
    /// three strands of four, never one strand of twelve.
    #[test]
    fn repeat_emits_one_span_per_instance_all_naming_one_object() {
        let resolved = resolve_shape(repeated_sector(3));
        assert_eq!(resolved.spans.len(), 3);
        for (instance, span) in resolved.spans.iter().enumerate() {
            assert_eq!(span.object, 0);
            assert_eq!(span.start, instance as u32 * 4);
            assert_eq!(span.count, 4);
        }
        // Lamps still name the document object, so selection and the rail see
        // one object however many strands it wires.
        assert!(resolved.lamps.iter().all(|lamp| lamp.object == 0));
        // …and the per-object read-path merges the strands back.
        let whole = resolved.object_span(0).unwrap();
        assert_eq!((whole.start, whole.count), (0, 12));
        assert_eq!(resolved.object_span(1), None);
    }

    /// Composing with P3: a repeated *gapped* path keeps its jumper inert in
    /// every instance — the gap is a property of the inner shape, resolved
    /// once and rotated, so it cannot drift instance to instance.
    #[test]
    fn repeat_of_a_gapped_path_keeps_the_jumper_inert_in_every_instance() {
        let plain = resolve_shape(Map2dShape::Path(gapped_l(vec![1], 5, false)));
        let resolved = resolve_shape(Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(gapped_l(vec![1], 5, false))),
            center: [10.0, 5.0],
            count: 4,
        }));
        // 5 lamps per instance — the inert segment adds none, in any instance.
        assert_eq!(resolved.lamps.len(), 20);
        assert!(resolved.spans.iter().all(|span| span.count == 5));
        // Instance 2 is the half turn of instance 0 about [10, 5].
        for lamp in 0..5usize {
            let base = plain.lamps[lamp].pos;
            let want = [2.0 * 10.0 - base[0], 2.0 * 5.0 - base[1]];
            let actual = resolved.lamps[10 + lamp].pos;
            assert!(
                (actual[0] - want[0]).abs() < 1e-3 && (actual[1] - want[1]).abs() < 1e-3,
                "lamp {lamp}: {actual:?} != {want:?}"
            );
        }
    }

    /// Nesting multiplies strands: the innermost instances are the strands, so
    /// a 3-way repeat of a 2-way repeat of a path is six of them.
    #[test]
    fn nested_repeat_multiplies_the_strands() {
        let resolved = resolve_shape(Map2dShape::Repeat(RepeatShape {
            shape: Box::new(repeated_sector(2)),
            center: [100.0, 100.0],
            count: 3,
        }));
        assert_eq!(resolved.spans.len(), 6);
        assert_eq!(resolved.lamps.len(), 24);
        for (strand, span) in resolved.spans.iter().enumerate() {
            assert_eq!(span.object, 0);
            assert_eq!(span.start, strand as u32 * 4);
            assert_eq!(span.count, 4);
        }
    }

    /// `count: 1` is the identity: same lamps, same single span as the inner
    /// shape alone — wrapping a shape must never change what it resolves to.
    #[test]
    fn a_repeat_of_one_is_the_inner_shape_unchanged() {
        let inner = square_corner_path();
        let plain = resolve_shape(Map2dShape::Path(inner.clone()));
        let wrapped = resolve_shape(Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(inner)),
            center: [-40.0, 90.0],
            count: 1,
        }));
        assert_eq!(wrapped.lamps, plain.lamps);
        assert_eq!(wrapped.spans, plain.spans);
    }

    /// Zero instances place no lamps — the same load-time error every other
    /// shape gives for a count it cannot satisfy, naming the object.
    #[test]
    fn a_repeat_of_zero_is_invalid() {
        let doc = Map2dDoc {
            objects: vec![object(repeated_sector(0))],
            ..Map2dDoc::new()
        };
        let error = resolve(&doc).unwrap_err();
        assert!(matches!(
            &error,
            Map2dError::InvalidObject { reason, .. } if reason.contains("repeat count")
        ));
    }

    /// An invalid *inner* shape is still the outer object's error: the report
    /// names the wiring index the author can find in the rail.
    #[test]
    fn an_invalid_inner_shape_reports_the_repeating_object() {
        let doc = Map2dDoc {
            objects: vec![
                object(Map2dShape::Path(PathShape {
                    points: vec![[0.0, 0.0], [1.0, 0.0]],
                    count: 2,
                    reversed: false,
                    gaps: Vec::new(),
                    align: PathAlign::On,
                })),
                object(Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(Map2dShape::Path(PathShape {
                        points: vec![[0.0, 0.0]],
                        count: 2,
                        reversed: false,
                        gaps: Vec::new(),
                        align: PathAlign::On,
                    })),
                    center: [0.0, 0.0],
                    count: 3,
                })),
            ],
            ..Map2dDoc::new()
        };
        assert!(matches!(
            resolve(&doc).unwrap_err(),
            Map2dError::InvalidObject { object: 1, .. }
        ));
    }

    #[test]
    fn spans_track_object_ranges() {
        let doc = Map2dDoc {
            objects: vec![
                object(Map2dShape::Path(PathShape {
                    points: vec![[0.0, 0.0], [1.0, 0.0]],
                    count: 3,
                    reversed: false,
                    gaps: Vec::new(),
                    align: PathAlign::On,
                })),
                object(Map2dShape::Ring(button_rings())),
            ],
            ..Map2dDoc::new()
        };
        let resolved = resolve(&doc).unwrap();
        assert_eq!(resolved.spans.len(), 2);
        assert_eq!(resolved.spans[0].start, 0);
        assert_eq!(resolved.spans[0].count, 3);
        assert_eq!(resolved.spans[1].start, 3);
        assert_eq!(resolved.spans[1].count, 24);
        assert_eq!(resolved.lamps[3].object, 1);
    }

    #[test]
    fn invalid_objects_name_their_wiring_index() {
        let doc = Map2dDoc {
            objects: vec![Map2dObject {
                name: String::from("bad"),
                id: None,
                stride: None,
                shape: Map2dShape::Path(PathShape {
                    points: vec![[0.0, 0.0]],
                    count: 2,
                    reversed: false,
                    gaps: Vec::new(),
                    align: PathAlign::On,
                }),
            }],
            ..Map2dDoc::new()
        };
        let error = resolve(&doc).unwrap_err();
        assert!(matches!(error, Map2dError::InvalidObject { object: 0, .. }));
    }

    #[test]
    fn resolution_is_deterministic() {
        let doc = Map2dDoc {
            objects: vec![object(Map2dShape::Ring(button_rings()))],
            ..Map2dDoc::new()
        };
        assert_eq!(resolve(&doc).unwrap(), resolve(&doc).unwrap());
    }

    fn resolve_shape(shape: Map2dShape) -> ResolvedMap2d {
        let doc = Map2dDoc {
            objects: vec![object(shape)],
            ..Map2dDoc::new()
        };
        resolve(&doc).unwrap()
    }

    fn object(shape: Map2dShape) -> Map2dObject {
        Map2dObject {
            name: String::new(),
            id: None,
            stride: None,
            shape,
        }
    }

    /// Three 10-unit segments in an L-and-back: `[0,0] → [10,0] → [10,10] →
    /// [20,10]`. Every gap test reads off the same geometry.
    fn gapped_l(gaps: Vec<u32>, count: u32, reversed: bool) -> PathShape {
        PathShape {
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [20.0, 10.0]],
            count,
            reversed,
            gaps,
            align: PathAlign::On,
        }
    }

    /// A four-lamp diagonal well off the origin — deliberately asymmetric, so
    /// a rotation that silently did nothing would show.
    fn square_corner_path() -> PathShape {
        PathShape {
            points: vec![[30.0, 12.0], [90.0, 42.0]],
            count: 4,
            reversed: false,
            gaps: Vec::new(),
            align: PathAlign::On,
        }
    }

    /// `instances` copies of a four-lamp radial rib around the origin — the
    /// small-dome shape the span tests read off.
    fn repeated_sector(instances: u32) -> Map2dShape {
        Map2dShape::Repeat(RepeatShape {
            shape: Box::new(Map2dShape::Path(PathShape {
                points: vec![[0.0, -40.0], [0.0, -100.0]],
                count: 4,
                reversed: false,
                gaps: Vec::new(),
                align: PathAlign::On,
            })),
            center: [0.0, 0.0],
            count: instances,
        })
    }

    /// Screen-space angle in degrees (y-down, so increasing = clockwise).
    fn angle_of(pos: [f32; 2]) -> f32 {
        libm::atan2f(pos[1], pos[0]) * 180.0 / core::f32::consts::PI
    }

    /// Fold a degree difference into `(-180, 180]`.
    fn wrap_degrees(degrees: f32) -> f32 {
        let mut degrees = degrees % 360.0;
        if degrees > 180.0 {
            degrees -= 360.0;
        }
        if degrees <= -180.0 {
            degrees += 360.0;
        }
        degrees
    }

    fn button_rings() -> RingShape {
        RingShape {
            center: [240.0, 200.0],
            radius: 90.0,
            outer_count: 16,
            rings: 2,
            counts: Vec::new(),
            order: RingOrder::OuterFirst,
            start_angle_deg: -90.0,
            dir: RingDir::Cw,
        }
    }

    fn radius_of(pos: [f32; 2]) -> f32 {
        libm::sqrtf((pos[0] - 240.0).powi(2) + (pos[1] - 200.0).powi(2))
    }
}

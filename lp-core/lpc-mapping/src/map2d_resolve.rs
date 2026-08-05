//! The single deterministic resolver: document → ordered lamps.
//!
//! Wiring order is primary: lamps are numbered end-to-end across objects in
//! document order, and DMX-style addresses are *derived* from that order by
//! auto-flow ([`LAMPS_PER_UNIVERSE`] RGB lamps per universe). Manual patching
//! is future work layered on top; the wiring order never changes for it.

use alloc::string::ToString;
use alloc::vec::Vec;

use crate::map2d_doc::{
    GridCorner, GridRouting, GridShape, Map2dDoc, Map2dShape, PathShape, RingDir, RingOrder,
    RingShape,
};
use crate::map2d_error::Map2dError;

/// RGB lamps per DMX universe (170 × 3 channels = 510 of 512).
pub const LAMPS_PER_UNIVERSE: u32 = 170;

/// DMX channels per RGB lamp.
pub const CHANNELS_PER_LAMP: u32 = 3;

/// Derived DMX-style address of one lamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LampAddress {
    /// Zero-based universe index.
    pub universe: u16,
    /// Zero-based first channel of the lamp within its universe.
    pub channel: u16,
}

/// One resolved lamp in doc space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedLamp {
    /// Zero-based wiring-order index across the whole document.
    pub index: u32,
    /// Wiring-order index of the owning object in `doc.objects`.
    pub object: u32,
    /// Position in doc space (fit to a render target separately).
    pub pos: [f32; 2],
    pub address: LampAddress,
}

/// The contiguous lamp span an object resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectSpan {
    pub object: u32,
    /// Zero-based wiring index of the object's first lamp.
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

    /// Number of universes the auto-flow occupies (0 for an empty document).
    pub fn universe_count(&self) -> u32 {
        (self.lamps.len() as u32).div_ceil(LAMPS_PER_UNIVERSE)
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
        let positions = match &object.shape {
            Map2dShape::Grid(grid) => resolve_grid(grid, &invalid)?,
            Map2dShape::Ring(ring) => resolve_ring(ring, &invalid)?,
            Map2dShape::Path(path) => resolve_path(path, &invalid)?,
        };
        for pos in positions {
            let index = lamps.len() as u32;
            lamps.push(ResolvedLamp {
                index,
                object: object_index,
                pos,
                address: address_of(index),
            });
        }
        spans.push(ObjectSpan {
            object: object_index,
            start,
            count: lamps.len() as u32 - start,
        });
    }
    Ok(ResolvedMap2d { lamps, spans })
}

fn address_of(index: u32) -> LampAddress {
    LampAddress {
        universe: (index / LAMPS_PER_UNIVERSE).min(u16::MAX as u32) as u16,
        channel: ((index % LAMPS_PER_UNIVERSE) * CHANNELS_PER_LAMP) as u16,
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
    use crate::map2d_doc::{Map2dObject, Map2dShape};
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

    /// Out-of-range gap indices name no segment; the document still resolves
    /// (the editor sanitizes them away, a hand-edit should not brick a load).
    #[test]
    fn out_of_range_gap_indices_are_ignored() {
        let with_junk = resolve_shape(Map2dShape::Path(gapped_l(vec![1, 9, 42], 5, false)));
        let clean = resolve_shape(Map2dShape::Path(gapped_l(vec![1], 5, false)));
        assert_eq!(with_junk.positions(), clean.positions());
    }

    #[test]
    fn addresses_flow_across_universe_boundaries() {
        let resolved = resolve_shape(Map2dShape::Grid(GridShape {
            origin: [0.0, 0.0],
            cols: 200, // crosses the 170-lamp universe boundary
            rows: 1,
            pitch: 1.0,
            routing: GridRouting::Raster,
            start_corner: GridCorner::Tl,
        }));
        assert_eq!(resolved.lamps.len(), 200);
        assert_eq!(
            resolved.lamps[169].address,
            LampAddress {
                universe: 0,
                channel: 169 * 3
            }
        );
        assert_eq!(
            resolved.lamps[170].address,
            LampAddress {
                universe: 1,
                channel: 0
            }
        );
        assert_eq!(resolved.universe_count(), 2);
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
                shape: Map2dShape::Path(PathShape {
                    points: vec![[0.0, 0.0]],
                    count: 2,
                    reversed: false,
                    gaps: Vec::new(),
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
        }
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

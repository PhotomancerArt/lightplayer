//! Object BODIES for the DIVED document canvas: the aligned outline plus
//! the voronoi lamp cells the Arrange view already draws, derived here
//! straight from the session's own document.
//!
//! Arrange builds the same bodies from a `FixtureRender`'s drawn points
//! (`arrange.rs::sprite_objects`); dived, the document and its resolution
//! are right there, so the grain comes from the resolver instead: ONE BODY
//! PER SPAN — a repeat's instances are separate strands of wire — cut again
//! at every jumper ([`path_gap_breaks`]), so no band ever bridges a break.
//!
//! This is what gives the `align` control its picture: change it in the
//! properties panel and the next render re-derives the outline and the cell
//! centers from the edited document.

use std::collections::BTreeMap;

use lpc_mapping::{Map2dDoc, Map2dShape, PathAlign, ResolvedMap2d, path_gap_breaks};

use super::cells::{LampCell, lamp_cells};
use super::hull::hull_path_d;
use super::outline::aligned_outline;

/// Reach of the band off the lamps, as a fraction of the strand pitch —
/// arrange.rs's `sprite_objects` derivation, kept in step by hand (it works
/// from sprite points, this from resolved lamps, so there is no shared
/// input to factor through).
const PITCH_REACH: f64 = 0.65;

/// Floor for the reach as a fraction of the doc's declared lamp footprint:
/// however sparse the strand, the band still clothes the lamp. Doc units
/// are arbitrary (the G1 scale ruling), so nothing absolute may appear.
const FOOTPRINT_REACH: f64 = 0.55;

/// One drawn body: a single physical strand-group of one object.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObjectBody {
    /// Index into `doc.objects` — selection and colour come from it.
    pub object: usize,
    /// Which instance of a repeated object this is (0 for a plain shape):
    /// the same ordinal the lamp dots hue by while tessellating.
    pub instance: usize,
    /// Outline loops, painted with fill-rule nonzero.
    pub outline: Vec<Vec<[f32; 2]>>,
    /// Voronoi cells, `lamp` re-indexed to the ABSOLUTE wiring index (so a
    /// cell keys and reads like the lamp dot it belongs to). Empty for the
    /// field kinds (grid, ring), which stay dots inside a neutral band.
    pub cells: Vec<LampCell>,
}

/// Every object body of a resolved document, in wiring order.
///
/// O(lamps) per call with the cell clipping's small constant — the canvas
/// derives it once per render pass, never inside an element closure.
pub(crate) fn object_bodies(doc: &Map2dDoc, resolved: &ResolvedMap2d) -> Vec<ObjectBody> {
    let mut instances: BTreeMap<u32, usize> = BTreeMap::new();
    let mut bodies = Vec::with_capacity(resolved.spans.len());
    for span in &resolved.spans {
        let instance = {
            let ordinal = instances.entry(span.object).or_insert(0);
            let instance = *ordinal;
            *ordinal += 1;
            instance
        };
        let Some(object) = doc.objects.get(span.object as usize) else {
            continue;
        };
        // A repeat rotates an inner shape: the innermost LEAF says how the
        // lamps run (and carries the `align` the panel edits).
        let mut shape = &object.shape;
        while let Map2dShape::Repeat(repeat) = shape {
            shape = &repeat.shape;
        }
        let (align, celled, closed, breaks) = match shape {
            Map2dShape::Path(path) => (path.align, true, false, path_gap_breaks(path)),
            Map2dShape::Polygon(polygon) => (polygon.align, true, true, Vec::new()),
            // Grid and ring lamps are a FIELD, not a ribbon: the neutral
            // on-path band, and their dots keep speaking for themselves.
            _ => (PathAlign::On, false, false, Vec::new()),
        };
        // Cut the span at each jumper: one strand per lit run, and the
        // absolute lamp index of every point alongside.
        let mut strands: Vec<Vec<[f32; 2]>> = Vec::new();
        let mut lamp_indices: Vec<usize> = Vec::new();
        let mut cursor = 0;
        for offset in breaks.into_iter().chain([span.count]) {
            if offset <= cursor || offset > span.count {
                continue;
            }
            let start = (span.start + cursor) as usize;
            let end = start + (offset - cursor) as usize;
            let run = resolved.lamps.get(start..end).unwrap_or_default();
            if !run.is_empty() {
                strands.push(run.iter().map(|lamp| lamp.pos).collect());
                lamp_indices.extend(run.iter().map(|lamp| lamp.index as usize));
            }
            cursor = offset;
        }
        if strands.is_empty() {
            continue;
        }
        // ONE reach for the whole body — outline and cells stand off the
        // lamps by the same amount, so they agree by construction.
        let reach = (body_pitch(&strands).map_or(0.0, |pitch| PITCH_REACH * pitch))
            .max(FOOTPRINT_REACH * f64::from(doc.sample_diameter))
            .max(f64::EPSILON) as f32;
        // A closed shape's outline wraps: repeat the first lamp so the band
        // is an annulus, not a ribbon with a mouth at the seam. The CELLS
        // stay on the open run — one cell per lamp, always.
        let outline_strands: Vec<Vec<[f32; 2]>> = if closed {
            strands
                .iter()
                .map(|strand| {
                    let mut wrapped = strand.clone();
                    wrapped.extend(strand.first().copied());
                    wrapped
                })
                .collect()
        } else {
            strands.clone()
        };
        let cells = if celled {
            lamp_cells(&strands, align, doc.sample_diameter)
                .into_iter()
                .filter(|cell| cell.polygon.len() >= 3)
                .filter_map(|cell| {
                    lamp_indices.get(cell.lamp).map(|index| LampCell {
                        lamp: *index,
                        polygon: cell.polygon,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let outline = aligned_outline(&outline_strands, align, reach);
        if outline.is_empty() && cells.is_empty() {
            continue;
        }
        bodies.push(ObjectBody {
            object: span.object as usize,
            instance,
            outline,
            cells,
        });
    }
    bodies
}

/// Median of the consecutive lamp gaps across a body's strands — its pitch
/// as resolved. `None` when no strand has two distinct lamps.
fn body_pitch(strands: &[Vec<[f32; 2]>]) -> Option<f64> {
    let mut gaps: Vec<f64> = strands
        .iter()
        .flat_map(|strand| {
            strand.windows(2).map(|pair| {
                let (dx, dy) = (
                    f64::from(pair[1][0] - pair[0][0]),
                    f64::from(pair[1][1] - pair[0][1]),
                );
                (dx * dx + dy * dy).sqrt()
            })
        })
        .filter(|gap| *gap > 1e-9)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(f64::total_cmp);
    Some(gaps[gaps.len() / 2])
}

/// Every loop of an outline as ONE `d`: subpaths, so a body is a single
/// element and the browser's nonzero fill rule does the merging. (The
/// fixture layer has the same three lines over its own sprite outlines.)
#[must_use]
pub(crate) fn loops_path_d(loops: &[Vec<[f32; 2]>]) -> String {
    let mut d = String::new();
    for polygon in loops.iter().filter(|polygon| polygon.len() >= 3) {
        if !d.is_empty() {
            d.push(' ');
        }
        d.push_str(&hull_path_d(polygon));
    }
    d
}

#[cfg(test)]
mod tests {
    use lpc_mapping::{Map2dObject, PathShape, PolygonShape, resolve};

    use super::super::outline::point_in_loops;
    use super::*;

    fn doc_with(shape: Map2dShape) -> Map2dDoc {
        Map2dDoc {
            objects: vec![Map2dObject {
                name: "o".into(),
                id: None,
                stride: None,
                shape,
            }],
            ..Map2dDoc::new()
        }
    }

    fn path(points: Vec<[f32; 2]>, count: u32, align: PathAlign, gaps: Vec<u32>) -> Map2dShape {
        Map2dShape::Path(PathShape {
            points,
            count,
            reversed: false,
            gaps,
            align,
        })
    }

    fn bodies_of(doc: &Map2dDoc) -> Vec<ObjectBody> {
        object_bodies(doc, &resolve(doc).expect("resolves"))
    }

    /// The whole point of the layer: alignment moves the band off the path,
    /// and the two sides are mirror images. A probe one reach off the line
    /// is inside exactly one of Inside/Outside.
    #[test]
    fn align_moves_the_band_and_the_cells_off_the_path() {
        let line = vec![[0.0_f32, 0.0], [100.0, 0.0]];
        let mut sides = Vec::new();
        for align in [PathAlign::Inside, PathAlign::Outside] {
            let doc = doc_with(path(line.clone(), 11, align, Vec::new()));
            let bodies = bodies_of(&doc);
            assert_eq!(bodies.len(), 1);
            let body = &bodies[0];
            let above = point_in_loops(&body.outline, [50.0, -4.0]);
            let below = point_in_loops(&body.outline, [50.0, 4.0]);
            assert!(above != below, "{align:?} band must pick one side");
            sides.push(above);
            // Cells follow the same side: their centers shift with the band.
            assert_eq!(body.cells.len(), 11);
        }
        assert!(sides[0] != sides[1], "Outside mirrors Inside");
        // On straddles the path: both probes land in the band.
        let doc = doc_with(path(line, 11, PathAlign::On, Vec::new()));
        let body = &bodies_of(&doc)[0];
        assert!(point_in_loops(&body.outline, [50.0, -1.0]));
        assert!(point_in_loops(&body.outline, [50.0, 1.0]));
    }

    /// Cells carry ABSOLUTE wiring indices, so a cell names the same lamp
    /// its dot does — across objects, too.
    #[test]
    fn cells_are_keyed_by_absolute_lamp_index() {
        let doc = Map2dDoc {
            objects: vec![
                Map2dObject {
                    name: "a".into(),
                    id: None,
                    stride: None,
                    shape: path(vec![[0.0, 0.0], [40.0, 0.0]], 5, PathAlign::On, Vec::new()),
                },
                Map2dObject {
                    name: "b".into(),
                    id: None,
                    stride: None,
                    shape: path(
                        vec![[0.0, 50.0], [40.0, 50.0]],
                        4,
                        PathAlign::On,
                        Vec::new(),
                    ),
                },
            ],
            ..Map2dDoc::new()
        };
        let bodies = bodies_of(&doc);
        assert_eq!(bodies.len(), 2);
        assert_eq!(
            bodies[0].cells.iter().map(|c| c.lamp).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            bodies[1].cells.iter().map(|c| c.lamp).collect::<Vec<_>>(),
            vec![5, 6, 7, 8]
        );
    }

    /// A jumper breaks the wire, so it must break the band: the gap's
    /// middle is outside every loop, while each lit run is covered.
    #[test]
    fn a_jumper_breaks_the_band() {
        // Three segments; the middle one is inert.
        let doc = doc_with(path(
            vec![[0.0, 0.0], [40.0, 0.0], [80.0, 0.0], [120.0, 0.0]],
            9,
            PathAlign::On,
            vec![1],
        ));
        let body = &bodies_of(&doc)[0];
        assert!(
            !point_in_loops(&body.outline, [60.0, 0.0]),
            "the jumper's middle must stay bare"
        );
        assert!(point_in_loops(&body.outline, [20.0, 0.0]));
        assert!(point_in_loops(&body.outline, [100.0, 0.0]));
    }

    /// A polygon's band is an ANNULUS: it wraps the seam and leaves the
    /// interior open (nonzero fill over the hole loop).
    #[test]
    fn a_polygon_body_is_an_annulus() {
        let doc = doc_with(Map2dShape::Polygon(PolygonShape {
            points: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
            count: 24,
            align: PathAlign::On,
        }));
        let body = &bodies_of(&doc)[0];
        assert!(
            !point_in_loops(&body.outline, [50.0, 50.0]),
            "the middle of a closed shape stays open"
        );
        assert!(
            point_in_loops(&body.outline, [0.0, 0.0]),
            "and the seam is covered"
        );
    }

    /// A repeat draws one body per INSTANCE, ordinals in wiring order —
    /// the grain the lamp dots already hue by.
    #[test]
    fn a_repeat_draws_one_body_per_instance() {
        let doc = doc_with(Map2dShape::Repeat(lpc_mapping::RepeatShape {
            center: [0.0, 0.0],
            count: 4,
            shape: Box::new(path(
                vec![[10.0, 0.0], [50.0, 0.0]],
                5,
                PathAlign::Inside,
                Vec::new(),
            )),
        }));
        let bodies = bodies_of(&doc);
        assert_eq!(bodies.len(), 4);
        for (index, body) in bodies.iter().enumerate() {
            assert_eq!(body.object, 0);
            assert_eq!(body.instance, index);
            assert_eq!(body.cells.len(), 5);
            assert!(!body.outline.is_empty());
        }
    }

    /// Field kinds keep their dots: a band for context, no cells.
    #[test]
    fn a_grid_gets_a_band_but_no_cells() {
        let doc = doc_with(Map2dShape::Grid(lpc_mapping::GridShape {
            origin: [0.0, 0.0],
            cols: 4,
            rows: 3,
            pitch: 10.0,
            routing: lpc_mapping::GridRouting::Snake,
            start_corner: lpc_mapping::GridCorner::default(),
        }));
        let body = &bodies_of(&doc)[0];
        assert!(body.cells.is_empty());
        assert!(!body.outline.is_empty());
    }
}

//! A format-11 fixture's segments: its runs of lamps, in lamp order, with
//! their lamp counts — never their positions.
//!
//! Frozen from `lp-core/lpc-mapping` at origin/main b9f862fb2: map2d formats
//! 1–5 as `Map2dDoc::from_json` reads them (format peek, then the shapes,
//! ids, and duplicate-id check), the resolver's validity checks
//! (`resolve_into`), its lamp counts (`shape_lamp_count`), and its strands
//! (`object_instance_spans_of`): one segment per object, one per repeat
//! instance (nested repeats outermost first), a ring set one, a gapped path
//! one. `PathPoints` fixtures (lamps listed inline, no map2d) have one
//! segment per path, in `first_channel` order.
//!
//! A document the engine refuses today lights nothing on the board; the
//! frozen landing refuses it too, by name, rather than inventing lamps.

use super::v11_files::{refused, scalar};
use super::v11_filled_polygon_cells::{FilledPolygon, distance, filled_polygon_cell_count};
use crate::json::JsonNode;
use crate::upgrade_error::UpgradeError;

/// The newest map2d format the frozen reader takes (`MAP2D_FORMAT`).
const MAP2D_FORMAT_FROZEN: u32 = 5;

/// Longest object id (`MAP2D_OBJECT_ID_MAX_LEN`).
const OBJECT_ID_MAX_LEN: usize = 24;

/// One run of a fixture's lamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct V11Segment {
    pub(crate) source: SegmentSource,
    /// The segment's first lamp, in the fixture's lamp order.
    pub(crate) first_lamp: u32,
    pub(crate) lamps: u32,
}

/// Where a segment comes from in the fixture's mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentSource {
    /// A map2d object, or one copy of a repeat object.
    Object {
        /// The object's index in the document (its wiring order).
        index: u32,
        /// The object's id, when it has one (format ≥ 3).
        id: Option<String>,
        /// The object's name (`""` when absent).
        name: String,
        /// Repeat-copy indices, outermost first; empty for a plain shape.
        instances: Vec<u32>,
    },
    /// One `PointList` path of a `PathPoints` fixture, by its key.
    PointList { key: u32, first_channel: u32 },
}

/// What a map2d file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Map2dSegments {
    /// A format 1–5 document's segments.
    Segments(Vec<V11Segment>),
    /// A document at format 6 or newer: already the new mapping format, read
    /// by nothing here.
    AlreadyFormat6,
}

/// The segments of the map2d document at `file`.
pub(crate) fn map2d_segments(file: &str, doc: &JsonNode) -> Result<Map2dSegments, UpgradeError> {
    let format = doc
        .get("format")
        .and_then(scalar::<u32>)
        .ok_or_else(|| refused(file, "it has no format; the engine cannot read it"))?;
    if format > MAP2D_FORMAT_FROZEN {
        return Ok(Map2dSegments::AlreadyFormat6);
    }
    if format == 0 {
        return Err(refused(
            file,
            "it declares format 0; the engine cannot read it",
        ));
    }
    let objects = match doc.get("objects") {
        None => &Vec::new(),
        Some(JsonNode::Array(objects)) => objects,
        Some(_) => return Err(refused(file, "\"objects\" is not an array")),
    };
    let mut ids: Vec<String> = Vec::new();
    let mut segments = Vec::new();
    let mut next_lamp = 0u32;
    for (index, object) in objects.iter().enumerate() {
        let index = index as u32;
        let name = match object.get("name") {
            None => String::new(),
            Some(name) => name
                .as_str()
                .ok_or_else(|| refused(file, &format!("object {index}'s name is not a string")))?,
        };
        let id = match object.get("id") {
            None => None,
            Some(id) => {
                let id = id.as_str().ok_or_else(|| {
                    refused(file, &format!("object {index}'s id is not a string"))
                })?;
                check_object_id(&id).map_err(|reason| refused(file, &reason))?;
                if ids.contains(&id) {
                    return Err(refused(
                        file,
                        &format!("two objects claim the id {id:?}; the engine refuses it"),
                    ));
                }
                ids.push(id.clone());
                Some(id)
            }
        };
        if let Some(stride) = object.get("stride")
            && scalar::<u32>(stride).is_none()
        {
            return Err(refused(
                file,
                &format!("object {index}'s stride is not a count"),
            ));
        }
        let shape = object
            .get("shape")
            .ok_or_else(|| refused(file, &format!("object {index} has no shape")))?;
        let invalid = |reason: &str| {
            refused(
                file,
                &format!(
                    "object {index} ({name:?}): {reason}; the engine refuses this mapping today"
                ),
            )
        };
        for (instances, lamps) in strands(shape, &invalid)? {
            segments.push(V11Segment {
                source: SegmentSource::Object {
                    index,
                    id: id.clone(),
                    name: name.clone(),
                    instances,
                },
                first_lamp: next_lamp,
                lamps,
            });
            next_lamp = next_lamp.saturating_add(lamps);
        }
    }
    Ok(Map2dSegments::Segments(segments))
}

/// The segments of a `PathPoints` fixture: one per non-empty path, in
/// `first_channel` order. The paths must tile the lamps from 0 with no gap
/// and no overlap, or a segment's lamps would not be its own.
pub(crate) fn path_points_segments(
    node: &str,
    paths: &[(u32, u32, u32)],
) -> Result<Vec<V11Segment>, UpgradeError> {
    let mut paths: Vec<&(u32, u32, u32)> =
        paths.iter().filter(|(_, _, lamps)| *lamps > 0).collect();
    paths.sort_by_key(|(_, first_channel, _)| *first_channel);
    let mut next = 0u32;
    let mut segments = Vec::new();
    for (key, first_channel, lamps) in paths {
        if *first_channel != next {
            return Err(refused(
                node,
                &format!(
                    "its path {key} starts at channel {first_channel}, but the paths before it end \
                     at {next}; hand-placed paths must tile the lamps with no gap or overlap"
                ),
            ));
        }
        segments.push(V11Segment {
            source: SegmentSource::PointList {
                key: *key,
                first_channel: *first_channel,
            },
            first_lamp: *first_channel,
            lamps: *lamps,
        });
        next = first_channel.saturating_add(*lamps);
    }
    Ok(segments)
}

/// `(instance path, lamps)` per strand of one shape, in wiring order: a leaf
/// is one strand; a repeat prefixes each inner strand with its copy index.
fn strands(
    shape: &JsonNode,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<Vec<(Vec<u32>, u32)>, UpgradeError> {
    let (kind, body) =
        shape_variant(shape).ok_or_else(|| invalid("its shape is not one known shape"))?;
    if kind == "repeat" {
        let count: u32 = field(body, "count", invalid)?;
        let _: [f32; 2] = point(body.get("center"), invalid)?;
        if count == 0 {
            return Err(invalid("repeat count must be at least 1"));
        }
        let inner = body
            .get("shape")
            .ok_or_else(|| invalid("its repeat holds no shape"))?;
        let inner = strands(inner, invalid)?;
        let mut out = Vec::with_capacity(inner.len() * count as usize);
        for instance in 0..count {
            for (path, lamps) in &inner {
                let mut instances = Vec::with_capacity(1 + path.len());
                instances.push(instance);
                instances.extend_from_slice(path);
                out.push((instances, *lamps));
            }
        }
        return Ok(out);
    }
    Ok(vec![(Vec::new(), leaf_lamp_count(kind, body, invalid)?)])
}

/// One leaf shape's lamp count (`shape_lamp_count`), after the resolver's
/// validity checks for that shape.
fn leaf_lamp_count(
    kind: &str,
    body: &JsonNode,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<u32, UpgradeError> {
    match kind {
        "grid" => {
            let _: [f32; 2] = point(body.get("origin"), invalid)?;
            let cols: u32 = field(body, "cols", invalid)?;
            let rows: u32 = field(body, "rows", invalid)?;
            let pitch: f32 = field(body, "pitch", invalid)?;
            check_enum(body, "routing", &["snake", "raster"], invalid)?;
            check_enum(body, "start_corner", &["tl", "tr", "bl", "br"], invalid)?;
            if cols == 0 || rows == 0 {
                return Err(invalid("grid needs at least one column and one row"));
            }
            if pitch <= 0.0 {
                return Err(invalid("grid pitch must be positive"));
            }
            Ok(cols * rows)
        }
        "ring" => {
            let _: [f32; 2] = point(body.get("center"), invalid)?;
            let radius: f32 = field(body, "radius", invalid)?;
            let outer_count: u32 = field(body, "outer_count", invalid)?;
            let rings: u32 = optional_field(body, "rings", invalid)?.unwrap_or(1);
            let counts: Vec<u32> = match body.get("counts") {
                None => Vec::new(),
                Some(counts) => scalar_array(counts, invalid)?,
            };
            let _: Option<f32> = optional_field(body, "start_angle_deg", invalid)?;
            check_enum(body, "order", &["outer_first", "inner_first"], invalid)?;
            check_enum(body, "dir", &["cw", "ccw"], invalid)?;
            if outer_count == 0 {
                return Err(invalid("ring outer_count must be at least 1"));
            }
            if radius <= 0.0 {
                return Err(invalid("ring radius must be positive"));
            }
            if rings == 0 {
                return Err(invalid("ring needs at least one ring"));
            }
            // `shape_lamp_count`'s ring arm, verbatim.
            let mut total = 0;
            for ring_index in 0..rings {
                let ring_radius = radius * (rings - ring_index) as f32 / rings as f32;
                total += counts
                    .get(ring_index as usize)
                    .copied()
                    .filter(|count| *count > 0)
                    .unwrap_or_else(|| derived_ring_count(outer_count, ring_radius, radius));
            }
            Ok(total)
        }
        "path" => {
            let points = points(body.get("points"), invalid)?;
            let count: u32 = field(body, "count", invalid)?;
            let reversed: bool = optional_field(body, "reversed", invalid)?.unwrap_or(false);
            let gaps: Vec<u32> = match body.get("gaps") {
                None => Vec::new(),
                Some(gaps) => scalar_array(gaps, invalid)?,
            };
            check_enum(body, "align", &["on", "inside", "outside"], invalid)?;
            if count == 0 {
                return Err(invalid("path count must be at least 1"));
            }
            if points.len() < 2 {
                return Err(invalid("path needs at least 2 points"));
            }
            let mut walk = points.clone();
            let inert = inert_segments(points.len(), &gaps, reversed);
            if reversed {
                walk.reverse();
            }
            if active_length(&walk, &inert) <= f32::EPSILON {
                return Err(invalid(if gaps.is_empty() {
                    "path has zero length"
                } else {
                    "path has no active length (every segment is a gap)"
                }));
            }
            Ok(count)
        }
        "polygon" => {
            let points = points(body.get("points"), invalid)?;
            let count: u32 = field(body, "count", invalid)?;
            check_enum(body, "align", &["on", "inside", "outside"], invalid)?;
            if count == 0 {
                return Err(invalid("polygon count must be at least 1"));
            }
            if points.len() < 3 {
                return Err(invalid("polygon needs at least 3 points"));
            }
            let mut closed = points.clone();
            closed.push(points[0]);
            if active_length(&closed, &[]) <= f32::EPSILON {
                return Err(invalid("polygon has zero perimeter"));
            }
            Ok(count)
        }
        "filled_polygon" => {
            let filled = FilledPolygon {
                points: points(body.get("points"), invalid)?,
                pitch: field(body, "pitch", invalid)?,
                angle_deg: optional_field(body, "angle_deg", invalid)?.unwrap_or(0.0),
                origin: match body.get("origin") {
                    None => [0.0, 0.0],
                    Some(origin) => point(Some(origin), invalid)?,
                },
            };
            check_enum(body, "routing", &["snake", "raster"], invalid)?;
            check_enum(body, "start_corner", &["tl", "tr", "bl", "br"], invalid)?;
            if filled.points.len() < 3 {
                return Err(invalid("filled polygon needs at least 3 points"));
            }
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
            let cells = filled_polygon_cell_count(&filled);
            if cells == 0 {
                return Err(invalid("filled polygon contains no lattice cells"));
            }
            Ok(cells)
        }
        other => Err(invalid(&format!("unknown shape {other:?}"))),
    }
}

/// Inner ring counts scale with circumference: `max(1, round(outer * r / R))`
/// (`derived_ring_count`, verbatim).
fn derived_ring_count(outer_count: u32, radius: f32, outer_radius: f32) -> u32 {
    let scaled = libm::roundf(outer_count as f32 * radius / outer_radius);
    (scaled as u32).max(1)
}

/// Per-segment inert flags in walk order (`inert_segments`, verbatim).
fn inert_segments(points: usize, gaps: &[u32], reversed: bool) -> Vec<bool> {
    let segments = points.saturating_sub(1);
    let mut inert = vec![false; segments];
    for gap in gaps {
        let index = *gap as usize;
        if index < segments {
            let index = if reversed {
                segments - 1 - index
            } else {
                index
            };
            inert[index] = true;
        }
    }
    inert
}

/// Summed length of the segments that carry lamps (`active_length`).
fn active_length(points: &[[f32; 2]], inert: &[bool]) -> f32 {
    points
        .windows(2)
        .enumerate()
        .filter(|(index, _)| !inert.get(*index).copied().unwrap_or(false))
        .map(|(_, pair)| distance(pair[0], pair[1]))
        .sum()
}

/// The `{"kind": {…}}` of an externally tagged shape.
fn shape_variant(shape: &JsonNode) -> Option<(&str, &JsonNode)> {
    match shape.object()?.as_slice() {
        [(kind, body)] => Some((kind.as_str(), body)),
        _ => None,
    }
}

/// An object id's rules (`Map2dObjectId::new`).
pub(crate) fn check_object_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err(String::from("an object id is empty"));
    }
    if id.len() > OBJECT_ID_MAX_LEN {
        return Err(format!(
            "object id {id:?} is longer than {OBJECT_ID_MAX_LEN} bytes"
        ));
    }
    let mut chars = id.chars();
    let first = chars.next().unwrap_or(' ');
    if !first.is_ascii_lowercase() {
        return Err(format!(
            "object id {id:?} must start with a lowercase letter"
        ));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
        return Err(format!(
            "object id {id:?} may only contain [a-z0-9_-] after the first letter"
        ));
    }
    Ok(())
}

fn field<T: serde::de::DeserializeOwned>(
    body: &JsonNode,
    key: &str,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<T, UpgradeError> {
    optional_field(body, key, invalid)?.ok_or_else(|| invalid(&format!("it has no {key}")))
}

fn optional_field<T: serde::de::DeserializeOwned>(
    body: &JsonNode,
    key: &str,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<Option<T>, UpgradeError> {
    match body.get(key) {
        None => Ok(None),
        Some(value) => scalar(value)
            .map(Some)
            .ok_or_else(|| invalid(&format!("its {key} is not readable"))),
    }
}

fn scalar_array<T: serde::de::DeserializeOwned>(
    node: &JsonNode,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<Vec<T>, UpgradeError> {
    let JsonNode::Array(items) = node else {
        return Err(invalid("a list is not an array"));
    };
    items
        .iter()
        .map(|item| scalar(item).ok_or_else(|| invalid("a list item is not readable")))
        .collect()
}

fn point(
    node: Option<&JsonNode>,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<[f32; 2], UpgradeError> {
    match node {
        Some(JsonNode::Array(items)) if items.len() == 2 => {
            match (scalar::<f32>(&items[0]), scalar::<f32>(&items[1])) {
                (Some(x), Some(y)) => Ok([x, y]),
                _ => Err(invalid("a point is not two numbers")),
            }
        }
        _ => Err(invalid("a point is missing or not [x, y]")),
    }
}

fn points(
    node: Option<&JsonNode>,
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<Vec<[f32; 2]>, UpgradeError> {
    let Some(JsonNode::Array(items)) = node else {
        return Err(invalid("its points are missing or not a list"));
    };
    items
        .iter()
        .map(|item| point(Some(item), invalid))
        .collect()
}

fn check_enum(
    body: &JsonNode,
    key: &str,
    allowed: &[&str],
    invalid: &impl Fn(&str) -> UpgradeError,
) -> Result<(), UpgradeError> {
    match body.get(key) {
        None => Ok(()),
        Some(value) => match value.as_str() {
            Some(text) if allowed.contains(&text.as_str()) => Ok(()),
            _ => Err(invalid(&format!("its {key} is not one of {allowed:?}"))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(text: &str) -> Vec<(u32, u32, Vec<u32>)> {
        let doc = JsonNode::parse(text).unwrap();
        let Map2dSegments::Segments(segments) = map2d_segments("m.map2d.json", &doc).unwrap()
        else {
            panic!("format 6")
        };
        segments
            .into_iter()
            .map(|segment| {
                let SegmentSource::Object { instances, .. } = segment.source else {
                    panic!("object")
                };
                (segment.first_lamp, segment.lamps, instances)
            })
            .collect()
    }

    /// One segment per object, in object order, lamps end to end.
    #[test]
    fn each_object_is_one_segment_in_object_order() {
        let text = r#"{"format":1,"objects":[
            {"shape":{"grid":{"origin":[0,0],"cols":3,"rows":2,"pitch":1}}},
            {"shape":{"path":{"points":[[0,0],[1,0]],"count":5}}}
        ]}"#;
        assert_eq!(segments(text), vec![(0, 6, vec![]), (6, 5, vec![])]);
    }

    /// A repeat is one segment per copy; nested repeats list the outer copy
    /// first.
    #[test]
    fn a_nested_repeat_is_one_segment_per_copy_outermost_first() {
        let text = r#"{"format":2,"objects":[{"shape":{"repeat":{"center":[0,0],"count":2,
            "shape":{"repeat":{"center":[0,0],"count":3,
                "shape":{"path":{"points":[[1,0],[2,0]],"count":4}}}}}}}]}"#;
        let got = segments(text);
        assert_eq!(got.len(), 6);
        assert_eq!(got[0], (0, 4, vec![0, 0]));
        assert_eq!(got[1], (4, 4, vec![0, 1]));
        assert_eq!(got[3], (12, 4, vec![1, 0]));
    }

    /// A ring set is one segment: its rings' counts summed, a missing count
    /// derived from the circumference (`max(1, round(outer · r / R))`).
    #[test]
    fn a_ring_set_is_one_segment_with_derived_inner_counts() {
        let text = r#"{"format":1,"objects":[{"shape":{"ring":{"center":[0,0],"radius":10,
            "outer_count":12,"rings":3}}}]}"#;
        // Radii 10, 6.67, 3.33: 12 + round(8) + round(4).
        assert_eq!(segments(text), vec![(0, 24, vec![])]);
    }

    /// A gapped path is one segment.
    #[test]
    fn a_gapped_path_is_one_segment() {
        let text = r#"{"format":2,"objects":[{"shape":{"path":{
            "points":[[0,0],[10,0],[12,0],[20,0]],"count":9,"gaps":[1]}}}]}"#;
        assert_eq!(segments(text), vec![(0, 9, vec![])]);
    }

    /// A map2d the engine refuses (here: a zero-length path) is refused by
    /// name, not landed as lamps nobody can see.
    #[test]
    fn a_mapping_the_engine_refuses_is_refused() {
        let doc = JsonNode::parse(
            r#"{"format":1,"objects":[{"name":"dot","shape":{"path":{"points":[[1,1],[1,1]],"count":3}}}]}"#,
        )
        .unwrap();
        let error = map2d_segments("m.map2d.json", &doc).unwrap_err();
        assert!(error.to_string().contains("zero length"), "{error}");
    }

    /// Format 6 is already the new format.
    #[test]
    fn a_format_6_document_is_passed_through() {
        let doc = JsonNode::parse(r#"{"format":6,"parts":{}}"#).unwrap();
        assert_eq!(
            map2d_segments("m.map2d.json", &doc).unwrap(),
            Map2dSegments::AlreadyFormat6
        );
    }

    /// Hand-placed paths are segments in `first_channel` order and must tile
    /// the lamps.
    #[test]
    fn point_list_paths_tile_the_lamps_in_first_channel_order() {
        let got = path_points_segments("f.json", &[(0, 16, 16), (1, 0, 16)]).unwrap();
        assert_eq!(
            got.iter()
                .map(|segment| (segment.first_lamp, segment.lamps))
                .collect::<Vec<_>>(),
            vec![(0, 16), (16, 16)]
        );
        assert!(path_points_segments("f.json", &[(0, 0, 16), (1, 20, 4)]).is_err());
        assert!(path_points_segments("f.json", &[(0, 0, 16), (1, 10, 4)]).is_err());
    }
}

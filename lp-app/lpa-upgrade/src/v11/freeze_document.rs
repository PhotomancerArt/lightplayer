//! A format-11 fixture's mapping written as one map2d format-6 document:
//! its parts and its frozen patching (G1, 2026-10-27: one file).
//!
//! The bytes are the format-6 writer's one canonical form
//! (`lpc_mapping::map2d6::write`, frozen from origin/main a35e3cd1f): pretty
//! JSON ending in a newline, fields in the writer's order, each left out
//! when it holds its default, numbers spelled as `serde_json` spells an
//! `f32` (`10.0`, `0.1`), and a path's `d` as the writer writes it (each
//! number the shortest text that reads back to the same `f32`). So a
//! document the step writes reads and writes back byte for byte, and every
//! number reads back to the `f32` today's reader read.
//!
//! Each object becomes a part: its id (minted where it has none,
//! `freeze_ids`), its name, its shape under `kind` with today's values:
//!
//! - a `path`'s `points` and `gaps` become `d`: a move to the first point,
//!   a line to each next one, and a move instead of the line that ends a
//!   jumper (a gap index past the last segment is ignored, as today);
//! - a `ring` (today's ring set) becomes a `circle` with `split: "one"`
//!   (one segment for all its rings): ring k at today's radius
//!   `radius · (rings − k) / rings` in today's arithmetic order, with its
//!   `counts` entry or today's derived count, keys `r1`, `r2`…, `next_ring`
//!   past them, its start angle, direction, and order as `drawn_from`;
//! - a `repeat` becomes a repeat part (`repeat<n>`) holding the part, the
//!   object's id and name on the part it holds;
//! - a grid, a polygon and a filled polygon carry their fields over.
//!
//! An object's `stride` (the Patch tab's rotation step, format 3) has no
//! place in format 6 and is dropped; it never moved a lamp. Nothing about
//! lamps is written (DD12).

use super::freeze_fit::ExactFit;
use super::freeze_ids::{ObjectToName, PartIds};
use super::freeze_patching::{FrozenAddress, FrozenOverride, FrozenPatching};
use super::v11_files::{refused, scalar};
use crate::json::JsonNode;
use crate::upgrade_error::UpgradeError;

/// The format the step writes.
pub(crate) const MAP2D6_FORMAT: u32 = 6;

/// A map2d's default sample diameter (`DEFAULT_SAMPLE_DIAMETER`).
const DEFAULT_SAMPLE_DIAMETER: f32 = 2.0;

/// A ring set's default start angle (12 o'clock).
const DEFAULT_START_ANGLE_DEG: f32 = -90.0;

/// The name the document gives an output (its authored or minted name).
pub(crate) type OutputNameOf<'a> = &'a dyn Fn(&str) -> String;

/// What the id minting needs of each object of a format 1–5 map2d.
pub(crate) fn objects_to_name<'a>(
    file: &str,
    doc: &'a JsonNode,
    ids: &'a [Option<String>],
    names: &'a [String],
) -> Result<Vec<ObjectToName<'a>>, UpgradeError> {
    objects(file, doc)?
        .iter()
        .enumerate()
        .map(|(index, object)| {
            let mut shape = shape_of(file, object)?;
            let mut repeats = 0;
            while let ("repeat", body) = shape {
                repeats += 1;
                shape = variant(
                    file,
                    body.get("shape")
                        .ok_or_else(|| refused(file, "a repeat holds no shape"))?,
                )?;
            }
            Ok(ObjectToName {
                id: ids[index].as_deref(),
                name: &names[index],
                repeats,
                word: kind_word(file, shape.0)?,
            })
        })
        .collect()
}

/// Every object's `id` and `name` (`""` when absent), as authored.
pub(crate) fn object_ids_and_names(
    file: &str,
    doc: &JsonNode,
) -> Result<(Vec<Option<String>>, Vec<String>), UpgradeError> {
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for object in objects(file, doc)? {
        ids.push(object.get("id").and_then(JsonNode::as_str));
        names.push(
            object
                .get("name")
                .and_then(JsonNode::as_str)
                .unwrap_or_default(),
        );
    }
    Ok((ids, names))
}

/// Which objects carry a `stride`, by index.
pub(crate) fn strided_objects(file: &str, doc: &JsonNode) -> Result<Vec<usize>, UpgradeError> {
    Ok(objects(file, doc)?
        .iter()
        .enumerate()
        .filter(|(_, object)| object.get("stride").is_some())
        .map(|(index, _)| index)
        .collect())
}

/// A format 1–5 map2d as format 6, with this patching.
pub(crate) fn map2d6_document(
    file: &str,
    old: &JsonNode,
    ids: &PartIds,
    patching: &FrozenPatching,
    output_name: OutputNameOf<'_>,
) -> Result<JsonNode, UpgradeError> {
    let sample_diameter = match old.get("sample_diameter") {
        None => DEFAULT_SAMPLE_DIAMETER,
        Some(node) => f32_of(file, node, "sample_diameter")?,
    };
    let canvas = match old.get("canvas") {
        None => None,
        Some(node) => Some(f32_array::<4>(file, node, "canvas")?),
    };
    let names = object_ids_and_names(file, old)?.1;
    let mut parts = Vec::new();
    for (index, object) in objects(file, old)?.iter().enumerate() {
        let shape = object
            .get("shape")
            .ok_or_else(|| refused(file, &format!("object {index} has no shape")))?;
        let object_ids = &ids.objects[index];
        parts.push(part(
            file,
            shape,
            &object_ids.repeats,
            &object_ids.held,
            &names[index],
        )?);
    }
    Ok(document(
        sample_diameter,
        canvas,
        &ids.next.iter().map(|(w, n)| (*w, *n)).collect::<Vec<_>>(),
        parts,
        patching,
        output_name,
    ))
}

/// A `PathPoints` fixture's lamps as a format-6 document: one `points` part
/// per path (`points1`, …), in `first_channel` order.
pub(crate) fn points_document(
    fit: &ExactFit,
    paths: &[u32],
    patching: &FrozenPatching,
    output_name: OutputNameOf<'_>,
) -> JsonNode {
    let mut parts = Vec::new();
    let mut at = fit.at.iter();
    for (index, lamps) in paths.iter().enumerate() {
        let positions: Vec<JsonNode> = at
            .by_ref()
            .take(*lamps as usize)
            .map(|point| f32s(point))
            .collect();
        parts.push(object_node(vec![
            ("id", JsonNode::string(&format!("points{}", index + 1))),
            (
                "kind",
                object_node(vec![(
                    "points",
                    object_node(vec![("at", JsonNode::Array(positions))]),
                )]),
            ),
        ]));
    }
    let next: Vec<(&str, u32)> = if paths.is_empty() {
        Vec::new()
    } else {
        vec![("points", paths.len() as u32 + 1)]
    };
    document(
        fit.sample_diameter,
        Some(fit.canvas),
        &next,
        parts,
        patching,
        output_name,
    )
}

/// The top level, in the writer's order.
fn document(
    sample_diameter: f32,
    canvas: Option<[f32; 4]>,
    next: &[(&str, u32)],
    parts: Vec<JsonNode>,
    patching: &FrozenPatching,
    output_name: OutputNameOf<'_>,
) -> JsonNode {
    let mut members = vec![
        ("format", JsonNode::u32(MAP2D6_FORMAT)),
        ("sample_diameter", f32_node(sample_diameter)),
    ];
    if let Some(canvas) = canvas {
        members.push(("canvas", f32s(&canvas)));
    }
    if !next.is_empty() {
        let mut sorted = next.to_vec();
        sorted.sort();
        members.push((
            "next",
            object_node(
                sorted
                    .into_iter()
                    .map(|(word, n)| (word, JsonNode::u32(n)))
                    .collect(),
            ),
        ));
    }
    if let Some(start) = &patching.start {
        members.push(("start", start_node(start, output_name)));
    }
    members.push(("parts", JsonNode::Array(parts)));
    let mut overrides: Vec<&(String, FrozenOverride)> = patching.overrides.iter().collect();
    overrides.sort_by(|a, b| a.0.cmp(&b.0));
    if !overrides.is_empty() {
        members.push((
            "overrides",
            JsonNode::Object(
                overrides
                    .into_iter()
                    .map(|(id, entry)| (id.clone(), override_node(entry, output_name)))
                    .collect(),
            ),
        ));
    }
    object_node(members)
}

fn start_node(start: &FrozenAddress, output_name: OutputNameOf<'_>) -> JsonNode {
    object_node(vec![
        ("output", set(JsonNode::string(&output_name(&start.output)))),
        ("port", set(JsonNode::u32(start.port))),
        ("channel", set(lamps(start.lamp))),
    ])
}

fn override_node(entry: &FrozenOverride, output_name: OutputNameOf<'_>) -> JsonNode {
    let mut members = Vec::new();
    if let Some(output) = &entry.output {
        members.push(("output", set(JsonNode::string(&output_name(output)))));
    }
    if let Some(port) = entry.port {
        members.push(("port", set(JsonNode::u32(port))));
    }
    if let Some(lamp) = entry.lamp {
        members.push(("channel", set(lamps(lamp))));
    }
    if entry.off {
        members.push(("off", JsonNode::Scalar(String::from("true"))));
    }
    if entry.reverse {
        members.push(("direction", JsonNode::string("reverse")));
    }
    if entry.offset > 0 {
        members.push(("offset", JsonNode::u32(entry.offset)));
    }
    object_node(members)
}

fn set(value: JsonNode) -> JsonNode {
    object_node(vec![("set", value)])
}

fn lamps(n: u32) -> JsonNode {
    object_node(vec![("lamps", JsonNode::u32(n))])
}

/// One object's shape as a part, wrapped in its repeats (outermost first).
fn part(
    file: &str,
    shape: &JsonNode,
    repeat_ids: &[String],
    held: &str,
    name: &str,
) -> Result<JsonNode, UpgradeError> {
    let (kind, body) = variant(file, shape)?;
    if kind == "repeat" {
        let (id, inner_ids) = repeat_ids
            .split_first()
            .ok_or_else(|| refused(file, "a repeat has no id to take"))?;
        let inner = body
            .get("shape")
            .ok_or_else(|| refused(file, "a repeat holds no shape"))?;
        let held_part = part(file, inner, inner_ids, held, name)?;
        return Ok(object_node(vec![
            ("id", JsonNode::string(id)),
            (
                "kind",
                object_node(vec![(
                    "repeat",
                    object_node(vec![
                        ("center", f32s(&point(file, body.get("center"))?)),
                        ("count", JsonNode::u32(u32_field(file, body, "count")?)),
                        ("part", held_part),
                    ]),
                )]),
            ),
        ]));
    }
    let (word, body) = leaf(file, kind, body)?;
    let mut members = vec![("id", JsonNode::string(held))];
    if !name.is_empty() {
        members.push(("name", JsonNode::string(name)));
    }
    members.push(("kind", object_node(vec![(word, body)])));
    Ok(object_node(members))
}

/// A leaf shape's format-6 kind word and body.
fn leaf(file: &str, kind: &str, body: &JsonNode) -> Result<(&'static str, JsonNode), UpgradeError> {
    Ok(match kind {
        "path" => ("path", path_body(file, body)?),
        "polygon" => {
            let mut members = vec![
                ("points", points_node(file, body.get("points"))?),
                ("count", JsonNode::u32(u32_field(file, body, "count")?)),
            ];
            push_word_unless(&mut members, body, "align", "on");
            ("polygon", object_node(members))
        }
        "ring" => ("circle", circle_body(file, body)?),
        "grid" => (
            "grid",
            object_node(vec![
                ("origin", f32s(&point(file, body.get("origin"))?)),
                ("cols", JsonNode::u32(u32_field(file, body, "cols")?)),
                ("rows", JsonNode::u32(u32_field(file, body, "rows")?)),
                ("pitch", f32_node(f32_field(file, body, "pitch")?)),
                (
                    "routing",
                    JsonNode::string(&word_or(body, "routing", "snake")),
                ),
                (
                    "start_corner",
                    JsonNode::string(&word_or(body, "start_corner", "tl")),
                ),
            ]),
        ),
        "filled_polygon" => {
            let mut members = vec![
                ("points", points_node(file, body.get("points"))?),
                ("pitch", f32_node(f32_field(file, body, "pitch")?)),
            ];
            if let Some(angle) = body.get("angle_deg") {
                let angle = f32_of(file, angle, "angle_deg")?;
                if angle != 0.0 {
                    members.push(("angle_deg", f32_node(angle)));
                }
            }
            if body.get("origin").is_some() {
                let origin = point(file, body.get("origin"))?;
                if origin[0] != 0.0 || origin[1] != 0.0 {
                    members.push(("origin", f32s(&origin)));
                }
            }
            push_word_unless(&mut members, body, "routing", "snake");
            push_word_unless(&mut members, body, "start_corner", "tl");
            ("filled_polygon", object_node(members))
        }
        other => return Err(refused(file, &format!("unknown shape {other:?}"))),
    })
}

/// A path: `points` and `gaps` as `d` (module doc).
fn path_body(file: &str, body: &JsonNode) -> Result<JsonNode, UpgradeError> {
    let points = points_of(file, body.get("points"))?;
    let gaps: Vec<u32> = match body.get("gaps") {
        None => Vec::new(),
        Some(JsonNode::Array(gaps)) => gaps
            .iter()
            .map(|gap| scalar::<u32>(gap).ok_or_else(|| refused(file, "a path gap is not a count")))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(refused(file, "a path's gaps are not a list")),
    };
    let mut d = String::new();
    for (index, [x, y]) in points.iter().enumerate() {
        // Line `index - 1` ends at this point; a gap there makes it a move.
        let letter = if index == 0 || gaps.contains(&(index as u32 - 1)) {
            'M'
        } else {
            'L'
        };
        if !d.is_empty() {
            d.push(' ');
        }
        d.push_str(&format!("{letter} {x} {y}"));
    }
    let mut members = vec![
        ("d", JsonNode::string(&d)),
        ("count", JsonNode::u32(u32_field(file, body, "count")?)),
    ];
    if body
        .get("reversed")
        .map(|flag| scalar::<bool>(flag).ok_or_else(|| refused(file, "reversed is not a boolean")))
        .transpose()?
        .unwrap_or(false)
    {
        members.push(("reversed", JsonNode::Scalar(String::from("true"))));
    }
    push_word_unless(&mut members, body, "align", "on");
    Ok(object_node(members))
}

/// Today's ring set as one circle, every lamp where it was (module doc).
fn circle_body(file: &str, body: &JsonNode) -> Result<JsonNode, UpgradeError> {
    let radius = f32_field(file, body, "radius")?;
    let outer_count = u32_field(file, body, "outer_count")?;
    let rings = match body.get("rings") {
        None => 1,
        Some(rings) => scalar::<u32>(rings).ok_or_else(|| refused(file, "rings is not a count"))?,
    };
    let counts: Vec<u32> = match body.get("counts") {
        None => Vec::new(),
        Some(JsonNode::Array(counts)) => counts
            .iter()
            .map(|count| {
                scalar::<u32>(count).ok_or_else(|| refused(file, "a ring count is not a count"))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(refused(file, "a ring set's counts are not a list")),
    };
    let start_angle = match body.get("start_angle_deg") {
        None => DEFAULT_START_ANGLE_DEG,
        Some(angle) => f32_of(file, angle, "start_angle_deg")?,
    };
    let drawn_from = match word_or(body, "order", "outer_first").as_str() {
        "inner_first" => "inside",
        _ => "outside",
    };
    let mut ring_nodes = Vec::with_capacity(rings as usize);
    for k in 0..rings {
        // `RingShape`'s ring radius, in today's arithmetic order.
        let ring_radius = radius * (rings - k) as f32 / rings as f32;
        let count = counts
            .get(k as usize)
            .copied()
            .filter(|count| *count > 0)
            .unwrap_or_else(|| derived_ring_count(outer_count, ring_radius, radius));
        ring_nodes.push(object_node(vec![
            ("key", JsonNode::string(&format!("r{}", k + 1))),
            ("radius", f32_node(ring_radius)),
            ("count", JsonNode::u32(count)),
        ]));
    }
    Ok(object_node(vec![
        ("center", f32s(&point(file, body.get("center"))?)),
        ("start_angle_deg", f32_node(start_angle)),
        ("dir", JsonNode::string(&word_or(body, "dir", "cw"))),
        ("drawn_from", JsonNode::string(drawn_from)),
        ("split", JsonNode::string("one")),
        ("next_ring", JsonNode::u32(rings + 1)),
        ("rings", JsonNode::Array(ring_nodes)),
    ]))
}

/// `max(1, round(outer · r / R))` (`derived_ring_count`, verbatim).
fn derived_ring_count(outer_count: u32, radius: f32, outer_radius: f32) -> u32 {
    let scaled = libm::roundf(outer_count as f32 * radius / outer_radius);
    (scaled as u32).max(1)
}

/// A format 1–5 shape's kind word as a part's (`ring` is a circle,
/// `filled_polygon` is `filled`).
fn kind_word(file: &str, kind: &str) -> Result<&'static str, UpgradeError> {
    Ok(match kind {
        "path" => "path",
        "polygon" => "polygon",
        "ring" => "circle",
        "grid" => "grid",
        "filled_polygon" => "filled",
        other => return Err(refused(file, &format!("unknown shape {other:?}"))),
    })
}

fn objects<'a>(file: &str, doc: &'a JsonNode) -> Result<&'a [JsonNode], UpgradeError> {
    match doc.get("objects") {
        None => Ok(&[]),
        Some(JsonNode::Array(objects)) => Ok(objects),
        Some(_) => Err(refused(file, "\"objects\" is not an array")),
    }
}

fn shape_of<'a>(file: &str, object: &'a JsonNode) -> Result<(&'a str, &'a JsonNode), UpgradeError> {
    variant(
        file,
        object
            .get("shape")
            .ok_or_else(|| refused(file, "an object has no shape"))?,
    )
}

/// The `{"kind": {…}}` of an externally tagged shape.
fn variant<'a>(file: &str, shape: &'a JsonNode) -> Result<(&'a str, &'a JsonNode), UpgradeError> {
    match shape.object().map(Vec::as_slice) {
        Some([(kind, body)]) => Ok((kind.as_str(), body)),
        _ => Err(refused(file, "a shape is not one known shape")),
    }
}

fn object_node(members: Vec<(&str, JsonNode)>) -> JsonNode {
    JsonNode::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

/// An `f32` as `serde_json` writes one.
fn f32_node(value: f32) -> JsonNode {
    JsonNode::Scalar(serde_json::to_string(&value).unwrap_or_else(|_| String::from("null")))
}

fn f32s(values: &[f32]) -> JsonNode {
    JsonNode::Array(values.iter().copied().map(f32_node).collect())
}

fn f32_of(file: &str, node: &JsonNode, what: &str) -> Result<f32, UpgradeError> {
    scalar::<f32>(node).ok_or_else(|| refused(file, &format!("{what} is not a number")))
}

fn f32_field(file: &str, body: &JsonNode, key: &str) -> Result<f32, UpgradeError> {
    f32_of(
        file,
        body.get(key)
            .ok_or_else(|| refused(file, &format!("a shape has no {key}")))?,
        key,
    )
}

fn u32_field(file: &str, body: &JsonNode, key: &str) -> Result<u32, UpgradeError> {
    body.get(key)
        .and_then(scalar::<u32>)
        .ok_or_else(|| refused(file, &format!("a shape's {key} is not a count")))
}

fn f32_array<const N: usize>(
    file: &str,
    node: &JsonNode,
    what: &str,
) -> Result<[f32; N], UpgradeError> {
    match node {
        JsonNode::Array(items) if items.len() == N => {
            let mut out = [0.0f32; N];
            for (slot, item) in out.iter_mut().zip(items) {
                *slot = f32_of(file, item, what)?;
            }
            Ok(out)
        }
        _ => Err(refused(file, &format!("{what} is not {N} numbers"))),
    }
}

fn point(file: &str, node: Option<&JsonNode>) -> Result<[f32; 2], UpgradeError> {
    f32_array::<2>(
        file,
        node.ok_or_else(|| refused(file, "a point is missing"))?,
        "a point",
    )
}

fn points_of(file: &str, node: Option<&JsonNode>) -> Result<Vec<[f32; 2]>, UpgradeError> {
    match node {
        Some(JsonNode::Array(items)) => items.iter().map(|item| point(file, Some(item))).collect(),
        _ => Err(refused(file, "a shape's points are missing or not a list")),
    }
}

fn points_node(file: &str, node: Option<&JsonNode>) -> Result<JsonNode, UpgradeError> {
    Ok(JsonNode::Array(
        points_of(file, node)?
            .iter()
            .map(|point| f32s(point))
            .collect(),
    ))
}

/// An enum word, or its default when absent.
fn word_or(body: &JsonNode, key: &str, default: &str) -> String {
    body.get(key)
        .and_then(JsonNode::as_str)
        .unwrap_or_else(|| String::from(default))
}

/// An enum word, written only when it is not the default.
fn push_word_unless<'a>(
    members: &mut Vec<(&'a str, JsonNode)>,
    body: &JsonNode,
    key: &'a str,
    default: &str,
) {
    let word = word_or(body, key, default);
    if word != default {
        members.push((key, JsonNode::string(&word)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v11::freeze_ids::mint_part_ids;

    /// A path with jumpers is one `d`, a move at each jumper.
    #[test]
    fn a_path_with_jumpers_becomes_one_d() {
        let body =
            JsonNode::parse(r#"{"points":[[0,0],[10,0.5],[12,0],[20,0]],"count":9,"gaps":[1,7]}"#)
                .unwrap();
        let path = path_body("m.json", &body).unwrap();
        assert_eq!(
            path.get("d").and_then(JsonNode::as_str).as_deref(),
            Some("M 0 0 L 10 0.5 M 12 0 L 20 0")
        );
    }

    /// A ring set is one circle, split one, with today's radii and counts.
    #[test]
    fn a_ring_set_becomes_one_circle_with_todays_rings() {
        let body = JsonNode::parse(
            r#"{"center":[0,0],"radius":10,"outer_count":12,"rings":3,"counts":[0,9]}"#,
        )
        .unwrap();
        let circle = circle_body("m.json", &body).unwrap();
        let text = String::from_utf8(circle.to_pretty_bytes()).unwrap();
        assert!(text.contains("\"split\": \"one\""), "{text}");
        assert!(text.contains("\"next_ring\": 4"), "{text}");
        let JsonNode::Array(rings) = circle.get("rings").unwrap() else {
            panic!("rings")
        };
        let counts: Vec<u32> = rings
            .iter()
            .map(|ring| ring.get("count").and_then(JsonNode::as_u32).unwrap())
            .collect();
        // Ring 0 derives 12, ring 1 is authored 9, ring 2 derives round(4).
        assert_eq!(counts, vec![12, 9, 4]);
    }

    /// Ids, kinds and defaults as the writer spells them.
    #[test]
    fn a_document_reads_like_the_writers() {
        let old = JsonNode::parse(
            r#"{"format":2,"sample_diameter":1,"objects":[
              {"name":"Rim A","shape":{"repeat":{"center":[0,0],"count":2,
                 "shape":{"grid":{"origin":[1,2],"cols":2,"rows":1,"pitch":1}}}}}]}"#,
        )
        .unwrap();
        let (ids, names) = object_ids_and_names("m.json", &old).unwrap();
        let ids = mint_part_ids(&objects_to_name("m.json", &old, &ids, &names).unwrap());
        let doc = map2d6_document("m.json", &old, &ids, &FrozenPatching::default(), &|node| {
            node.to_string()
        })
        .unwrap();
        let text = String::from_utf8(doc.to_pretty_bytes()).unwrap();
        assert_eq!(
            text,
            r#"{
  "format": 6,
  "sample_diameter": 1.0,
  "next": {
    "repeat": 2
  },
  "parts": [
    {
      "id": "repeat1",
      "kind": {
        "repeat": {
          "center": [
            0.0,
            0.0
          ],
          "count": 2,
          "part": {
            "id": "rim-a",
            "name": "Rim A",
            "kind": {
              "grid": {
                "origin": [
                  1.0,
                  2.0
                ],
                "cols": 2,
                "rows": 1,
                "pitch": 1.0,
                "routing": "snake",
                "start_corner": "tl"
              }
            }
          }
        }
      }
    }
  ]
}
"#
        );
    }
}

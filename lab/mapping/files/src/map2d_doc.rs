//! The product's mapping document (`*.map2d.json`), re-created: a fixture's
//! canvas and its objects, in wiring order. A copy of the shapes in
//! `lpc-mapping`'s `map2d_doc.rs`, not an import (the fence).
//!
//! The lab reads the shapes it models into components — a path (points,
//! count, `reversed`, `gaps`), a polygon, a repeat of either, and a ring
//! with one ring of lamps from 12 o'clock (one ring of a lab circle) — and
//! refuses the rest (grids, filled polygons, multi-ring rings) with a
//! reason, rather than opening a fixture with lamps missing.
//!
//! Every object keeps its JSON. Writing one back starts from it and changes
//! only what the lab changed, so a field the lab knows nothing about
//! (`align`, `stride`, a name) is kept, and a number that still means the
//! same (as f32) keeps the very text it was read from.
//!
//! The format gate is the product's: a document newer than format 5 is
//! refused, and a written one is stamped with the lowest format that holds
//! it (ids, strides and polygons need 3, gaps and repeats 2, an `align`
//! other than `on` 4).

use lab_mapping_model::component::Ring;
use lab_mapping_model::{ComponentKind, Vec2};
use serde_json::{Map, Value};

use crate::json::{f32_of, num_f32, point_f32, points_f32, read_point, set, u32_of};

pub const MAP2D_FORMAT: u32 = 5;

/// A document as read: its JSON, its canvas, and its objects.
#[derive(Debug, Clone)]
pub struct Map2dDoc {
    pub value: Value,
    /// `[min_x, min_y, width, height]`, when it has one.
    pub canvas: Option<[f64; 4]>,
    pub objects: Vec<MapObject>,
}

/// One object: its stable id (if the file gives one), name, and shape.
#[derive(Debug, Clone)]
pub struct MapObject {
    pub id: Option<String>,
    pub name: String,
    pub kind: ComponentKind,
    pub value: Value,
}

impl Map2dDoc {
    pub fn parse(text: &str) -> Result<Map2dDoc, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let format = value
            .get("format")
            .and_then(u32_of)
            .ok_or("no format number")?;
        if format == 0 || format > MAP2D_FORMAT {
            return Err(format!(
                "mapping format {format} is newer than this copy reads (up to {MAP2D_FORMAT})"
            ));
        }
        let canvas = match value.get("canvas") {
            None | Some(Value::Null) => None,
            Some(Value::Array(c)) if c.len() == 4 => {
                let n = |i: usize| f32_of(&c[i]).ok_or("canvas holds numbers");
                Some([n(0)?, n(1)?, n(2)?, n(3)?])
            }
            Some(_) => return Err("canvas is [min_x, min_y, width, height]".into()),
        };
        let mut objects = Vec::new();
        for (i, o) in value
            .get("objects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let name = o
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let shape = o.get("shape").ok_or(format!("object {i} has no shape"))?;
            let kind = kind_of(shape).map_err(|e| format!("object {i} ({name}): {e}"))?;
            objects.push(MapObject {
                id: o.get("id").and_then(Value::as_str).map(str::to_string),
                name,
                kind,
                value: o.clone(),
            });
        }
        Ok(Map2dDoc {
            value,
            canvas,
            objects,
        })
    }
}

/// A shape, as a lab component kind.
fn kind_of(shape: &Value) -> Result<ComponentKind, String> {
    let (tag, s) = tagged(shape)?;
    let points = |s: &Value| -> Result<Vec<Vec2>, String> {
        s.get("points")
            .and_then(Value::as_array)
            .ok_or("no points")?
            .iter()
            .map(|p| {
                read_point(p)
                    .map(|(x, y)| Vec2::new(x, y))
                    .ok_or_else(|| "a point is [x, y]".to_string())
            })
            .collect()
    };
    let count = |s: &Value| s.get("count").and_then(u32_of).ok_or("no count");
    match tag {
        "path" => Ok(ComponentKind::Path {
            points: points(s)?,
            lamps: count(s)?,
            reversed: s.get("reversed").and_then(Value::as_bool).unwrap_or(false),
            gaps: s
                .get("gaps")
                .and_then(Value::as_array)
                .map(|g| g.iter().filter_map(u32_of).collect())
                .unwrap_or_default(),
        }),
        "polygon" => Ok(ComponentKind::Polygon {
            points: points(s)?,
            lamps: count(s)?,
        }),
        "repeat" => {
            let inner = s.get("shape").ok_or("a repeat has no shape")?;
            let (x, y) = s
                .get("center")
                .and_then(read_point)
                .ok_or("a repeat has no center")?;
            Ok(ComponentKind::Repeat {
                shape: Box::new(kind_of(inner)?),
                center: Vec2::new(x, y),
                count: count(s)?,
            })
        }
        "ring" => ring_of(s),
        other => Err(format!("the lab can't open {other} shapes yet")),
    }
}

/// A product ring the lab can hold: one ring of lamps, lamp 0 at
/// 12 o'clock. More rings share one strand in the product, which a lab
/// circle (one object per ring) can't say without moving lamps.
fn ring_of(s: &Value) -> Result<ComponentKind, String> {
    let rings = s.get("rings").and_then(u32_of).unwrap_or(1);
    let start = s.get("start_angle_deg").and_then(f32_of).unwrap_or(-90.0);
    if rings != 1 || start != -90.0 || s.get("counts").is_some_and(|c| c != &Value::Array(vec![])) {
        return Err(
            "the lab opens a ring of one ring starting at 12 o'clock; this one has more".into(),
        );
    }
    let (x, y) = s
        .get("center")
        .and_then(read_point)
        .ok_or("a ring has no center")?;
    Ok(ComponentKind::Circle {
        center: Vec2::new(x, y),
        rings: vec![Ring {
            key: "r1".into(),
            radius: s
                .get("radius")
                .and_then(f32_of)
                .ok_or("a ring has no radius")?,
            lamps: s
                .get("outer_count")
                .and_then(u32_of)
                .ok_or("a ring has no outer_count")?,
            reversed: s.get("dir").and_then(Value::as_str) == Some("ccw"),
        }],
        next_ring: 2,
    })
}

fn tagged(shape: &Value) -> Result<(&str, &Value), String> {
    let obj = shape.as_object().ok_or("a shape is {\"kind\": {…}}")?;
    match obj.iter().next() {
        Some((tag, s)) if obj.len() == 1 => Ok((tag.as_str(), s)),
        _ => Err("a shape is {\"kind\": {…}}".into()),
    }
}

/// A component kind as a shape, starting from `old` (the shape it was read
/// from, if any) so what the lab doesn't know is kept. A circle of one ring
/// is one ring shape; `ring` picks which of a circle's rings.
pub fn shape_of(old: Option<&Value>, kind: &ComponentKind, ring: usize) -> Result<Value, String> {
    let old_inner = |tag: &str| -> Map<String, Value> {
        old.and_then(|o| o.get(tag))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let (tag, inner) = match kind {
        ComponentKind::Path {
            points,
            lamps,
            reversed,
            gaps,
        } => {
            let mut s = old_inner("path");
            let old_points = s.get("points").cloned();
            s.insert(
                "points".into(),
                points_f32(old_points.as_ref(), points.iter().map(|p| (p.x, p.y))),
            );
            s.insert("count".into(), (*lamps).into());
            let had = s.contains_key("reversed");
            set(
                &mut s,
                "reversed",
                (had || *reversed).then(|| (*reversed).into()),
            );
            let had = s.contains_key("gaps");
            set(
                &mut s,
                "gaps",
                (had || !gaps.is_empty()).then(|| gaps.clone().into()),
            );
            ("path", s)
        }
        ComponentKind::Polygon { points, lamps } => {
            let mut s = old_inner("polygon");
            let old_points = s.get("points").cloned();
            s.insert(
                "points".into(),
                points_f32(old_points.as_ref(), points.iter().map(|p| (p.x, p.y))),
            );
            s.insert("count".into(), (*lamps).into());
            ("polygon", s)
        }
        ComponentKind::Repeat {
            shape,
            center,
            count,
        } => {
            let mut s = old_inner("repeat");
            let old_shape = s.get("shape").cloned();
            s.insert("shape".into(), shape_of(old_shape.as_ref(), shape, 0)?);
            let old_center = s.get("center").cloned();
            s.insert(
                "center".into(),
                point_f32(old_center.as_ref(), center.x, center.y),
            );
            s.insert("count".into(), (*count).into());
            ("repeat", s)
        }
        ComponentKind::Circle { center, rings, .. } => {
            let r = rings.get(ring).ok_or("a circle with no rings")?;
            let mut s = old_inner("ring");
            let old_center = s.get("center").cloned();
            s.insert(
                "center".into(),
                point_f32(old_center.as_ref(), center.x, center.y),
            );
            let old_radius = s.get("radius").cloned();
            s.insert("radius".into(), num_f32(old_radius.as_ref(), r.radius));
            s.insert("outer_count".into(), r.lamps.into());
            let had = s.contains_key("dir");
            set(
                &mut s,
                "dir",
                (had || r.reversed).then(|| if r.reversed { "ccw" } else { "cw" }.into()),
            );
            ("ring", s)
        }
        ComponentKind::Group { .. } => return Err("a group is not one shape".into()),
    };
    let mut out = Map::new();
    out.insert(tag.into(), Value::Object(inner));
    Ok(Value::Object(out))
}

/// The lowest format that holds these objects: the product's
/// `required_format`.
pub fn required_format(objects: &[Value]) -> u32 {
    let mut format = 1;
    for o in objects {
        if o.get("id").is_some() || o.get("stride").is_some() {
            format = format.max(3);
        }
        if let Some(shape) = o.get("shape") {
            format = format.max(shape_format(shape));
        }
    }
    format
}

fn shape_format(shape: &Value) -> u32 {
    let Ok((tag, s)) = tagged(shape) else {
        return 1;
    };
    let aligned = s
        .get("align")
        .and_then(Value::as_str)
        .is_some_and(|a| a != "on");
    let align = if aligned { 4 } else { 1 };
    match tag {
        "path" => {
            let gaps = s
                .get("gaps")
                .and_then(Value::as_array)
                .is_some_and(|g| !g.is_empty());
            align.max(if gaps { 2 } else { 1 })
        }
        "polygon" => align.max(3),
        "filled_polygon" => 5,
        "repeat" => s.get("shape").map_or(2, |inner| shape_format(inner).max(2)),
        _ => 1,
    }
}

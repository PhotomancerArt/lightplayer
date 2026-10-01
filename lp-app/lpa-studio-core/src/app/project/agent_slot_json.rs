//! [`json_slot_edits`]: the app agent's `set` value, as the slot edits a
//! person's field edits would make.
//!
//! The model writes values the way they appear in a node's JSON file
//! (`"render_size": {"width": 250, "height": 8}`, `"cycle": {"kind":
//! "cycle", "step_seconds": 30}`). The overlay edits leaves, so the value is
//! walked through the slot's shape: records and maps fan out into their
//! fields and entries, an absent option or map entry is made present first
//! (`EnsurePresent`, the same gesture the editor's "+" makes), an enum's
//! `kind` picks its variant, and a leaf value is read through the shape's
//! own type — so a type mismatch is a readable rejection, never a panic.

use lpc_model::slot_codec::{JsonSyntaxSource, SlotReader, read_lp_value};
use lpc_model::{
    SlotEdit, SlotMapKey, SlotMapKeyShape, SlotName, SlotPath, SlotPathSegment, SlotShapeLookup,
    SlotShapeRegistry, SlotShapeView,
};
use serde_json::Value;

/// The leaf-level edits that write `value` at `path` in a def whose root
/// shape is `root`. `Err` names what did not fit, with the path.
pub(crate) fn json_slot_edits(
    registry: &SlotShapeRegistry,
    root: SlotShapeView<'_>,
    path: &SlotPath,
    value: &Value,
) -> Result<Vec<SlotEdit>, String> {
    let Some((shape, path)) = canonical_slot(registry, root, path) else {
        return Err(unresolved_path_message(registry, root, path));
    };
    let mut edits = Vec::new();
    walk(registry, shape, &path, value, &mut edits)?;
    Ok(edits)
}

/// The shape at `path` and the path as the overlay spells it: an option
/// the model stepped through without naming (`cycle.step_seconds`) gets its
/// `some` segment (`cycle.some.step_seconds`). `None` when the path does not
/// name a slot.
pub(crate) fn canonical_slot<'s>(
    registry: &'s SlotShapeRegistry,
    root: SlotShapeView<'s>,
    path: &SlotPath,
) -> Option<(SlotShapeView<'s>, SlotPath)> {
    let mut canonical = Vec::new();
    let shape = shape_at_into(registry, root, path.segments(), &mut canonical)?;
    Some((shape, SlotPath::from_segments(canonical)))
}

/// The shape at `segments`, or `None` when they do not name a slot.
fn shape_at<'s>(
    registry: &'s SlotShapeRegistry,
    shape: SlotShapeView<'s>,
    segments: &[SlotPathSegment],
) -> Option<SlotShapeView<'s>> {
    shape_at_into(registry, shape, segments, &mut Vec::new())
}

fn shape_at_into<'s>(
    registry: &'s SlotShapeRegistry,
    shape: SlotShapeView<'s>,
    segments: &[SlotPathSegment],
    canonical: &mut Vec<SlotPathSegment>,
) -> Option<SlotShapeView<'s>> {
    let shape = resolve(registry, shape)?;
    let Some((head, tail)) = segments.split_first() else {
        return Some(shape);
    };
    let next = match head {
        SlotPathSegment::Field(name) => {
            if let Some((_, field)) = shape.record_field_by_name(name) {
                field.shape()
            } else if name.as_str() == "some"
                && let Some(some) = shape.option_some()
            {
                some
            } else if let Some(variant) = shape.enum_variant_by_name(name) {
                variant.shape()
            } else if let Some(some) = shape.option_some() {
                // An option is transparent to the names inside it.
                canonical.push(SlotPathSegment::Field(SlotName::parse("some").ok()?));
                return shape_at_into(registry, some, segments, canonical);
            } else {
                return None;
            }
        }
        SlotPathSegment::Key(_) => shape.map_value()?,
    };
    canonical.push(head.clone());
    shape_at_into(registry, next, tail, canonical)
}

fn walk(
    registry: &SlotShapeRegistry,
    shape: SlotShapeView<'_>,
    path: &SlotPath,
    value: &Value,
    edits: &mut Vec<SlotEdit>,
) -> Result<(), String> {
    let shape = resolve(registry, shape)
        .ok_or_else(|| format!("`{path}` has a shape this build cannot resolve"))?;

    if let Some(some) = shape.option_some() {
        if value.is_null() {
            edits.push(SlotEdit::remove(path.clone()));
            return Ok(());
        }
        edits.push(SlotEdit::ensure_present(path.clone()));
        return walk(registry, some, &path.child(name("some")?), value, edits);
    }

    if let Some(ty) = shape
        .value_shape()
        .map(|value_shape| value_shape.ty_owned())
    {
        let text = value.to_string();
        let source =
            JsonSyntaxSource::new(&text).map_err(|error| format!("`{path}`: {error:?}"))?;
        let mut reader = SlotReader::new(source, registry);
        let lp_value = read_lp_value(&ty, reader.value())
            .map_err(|_| format!("`{path}` takes {}, not {value}", describe_type(&ty)))?;
        edits.push(SlotEdit::assign_value(path.clone(), lp_value));
        return Ok(());
    }

    if shape.is_enum() {
        let (variant_name, fields) = match value {
            Value::String(kind) => (kind.as_str(), None),
            Value::Object(map) => match map.get("kind").and_then(Value::as_str) {
                Some(kind) => (kind, Some(map)),
                None => {
                    return Err(format!(
                        "`{path}` is a choice: give its `kind` ({})",
                        variant_names(shape).join(", ")
                    ));
                }
            },
            other => {
                return Err(format!(
                    "`{path}` is a choice ({}), not {other}",
                    variant_names(shape).join(", ")
                ));
            }
        };
        let variant = find_variant(shape, variant_name).ok_or_else(|| {
            format!(
                "`{path}` has no kind {variant_name:?} (kinds: {})",
                variant_names(shape).join(", ")
            )
        })?;
        let variant_path = path.child(name(variant.name_str())?);
        edits.push(SlotEdit::ensure_present(variant_path.clone()));
        if let Some(fields) = fields {
            for (key, field_value) in fields.iter().filter(|(key, _)| *key != "kind") {
                let field_path = variant_path.child(name(key)?);
                let field_shape = shape_at(registry, variant.shape(), &[field_segment(key)?])
                    .ok_or_else(|| format!("`{path}` ({variant_name}) has no field {key:?}"))?;
                walk(registry, field_shape, &field_path, field_value, edits)?;
            }
        }
        return Ok(());
    }

    if let Some(len) = shape.record_fields_len() {
        let Value::Object(map) = value else {
            let fields: Vec<String> = (0..len)
                .filter_map(|index| shape.record_field(index))
                .map(|field| field.name_str().to_string())
                .collect();
            return Err(format!(
                "`{path}` is a group of fields ({}); give an object or set one field",
                fields.join(", ")
            ));
        };
        for (key, field_value) in map {
            let field_name = name(key)?;
            let Some((_, field)) = shape.record_field_by_name(&field_name) else {
                return Err(format!("`{path}` has no field {key:?}"));
            };
            walk(
                registry,
                field.shape(),
                &path.child(field_name),
                field_value,
                edits,
            )?;
        }
        return Ok(());
    }

    if let Some(entry_shape) = shape.map_value() {
        let Value::Object(map) = value else {
            return Err(format!("`{path}` is a map; give an object of entries"));
        };
        let key_shape = shape.map_key();
        for (key, entry_value) in map {
            let entry_path = path.child_key(map_key(key, key_shape, path)?);
            edits.push(SlotEdit::ensure_present(entry_path.clone()));
            walk(registry, entry_shape, &entry_path, entry_value, edits)?;
        }
        return Ok(());
    }

    Err(format!("`{path}` cannot be set from JSON"))
}

/// Why `path` does not resolve: the longest prefix that does, and what it
/// is — a whole value (set it whole), or a group without that field.
fn unresolved_path_message(
    registry: &SlotShapeRegistry,
    root: SlotShapeView<'_>,
    path: &SlotPath,
) -> String {
    let segments = path.segments();
    for len in (1..segments.len()).rev() {
        let Some(prefix_shape) = shape_at(registry, root, &segments[..len]) else {
            continue;
        };
        let prefix = SlotPath::from_segments(segments[..len].to_vec());
        if let Some(value_shape) = prefix_shape.value_shape() {
            return format!(
                "`{path}` is inside `{prefix}`, which is one value: set `{prefix}` whole ({})",
                describe_type(&value_shape.ty_owned())
            );
        }
        return format!(
            "`{prefix}` has no `{}`",
            SlotPath::from_segments(segments[len..].to_vec())
        );
    }
    format!("`{path}` is not a field of this node")
}

/// A type the way the model writes it in JSON.
fn describe_type(ty: &lpc_model::LpType) -> String {
    use lpc_model::LpType;
    match ty {
        LpType::String => "a string".to_string(),
        LpType::Bool => "true or false".to_string(),
        LpType::I32 | LpType::U32 => "a whole number".to_string(),
        LpType::F32 => "a number".to_string(),
        LpType::Vec2 | LpType::IVec2 => "[x, y]".to_string(),
        LpType::Vec3 => "[x, y, z]".to_string(),
        LpType::Vec4 => "[x, y, z, w]".to_string(),
        LpType::Struct { fields, .. } => format!(
            "an object {{{}}}",
            fields
                .iter()
                .map(|field| format!("{}: {}", field.name, describe_type(&field.ty)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => format!("a {other:?}"),
    }
}

/// Chase `Ref` indirections and `Custom` projections to a concrete shape.
fn resolve<'s>(
    registry: &'s SlotShapeRegistry,
    mut shape: SlotShapeView<'s>,
) -> Option<SlotShapeView<'s>> {
    for _ in 0..32 {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id)?;
        } else if let Some(projected) = shape.custom_shape() {
            shape = projected;
        } else {
            return Some(shape);
        }
    }
    None
}

fn name(text: &str) -> Result<SlotName, String> {
    SlotName::parse(text).map_err(|_| format!("{text:?} is not a field name"))
}

fn field_segment(text: &str) -> Result<SlotPathSegment, String> {
    Ok(SlotPathSegment::Field(name(text)?))
}

fn map_key(
    key: &str,
    shape: Option<SlotMapKeyShape>,
    path: &SlotPath,
) -> Result<SlotMapKey, String> {
    match shape {
        Some(SlotMapKeyShape::U32) => key
            .parse()
            .map(SlotMapKey::U32)
            .map_err(|_| format!("`{path}` is keyed by whole numbers, not {key:?}")),
        Some(SlotMapKeyShape::I32) => key
            .parse()
            .map(SlotMapKey::I32)
            .map_err(|_| format!("`{path}` is keyed by numbers, not {key:?}")),
        Some(SlotMapKeyShape::String) | None => Ok(SlotMapKey::String(key.to_string())),
    }
}

fn variant_names(shape: SlotShapeView<'_>) -> Vec<String> {
    (0..64)
        .map_while(|index| shape.enum_variant(index))
        .map(|variant| variant.name_str().to_string())
        .collect()
}

/// A variant by the name the JSON gives it (`Map2d`, `map2d`, `path_points`).
fn find_variant<'s>(
    shape: SlotShapeView<'s>,
    wanted: &str,
) -> Option<lpc_model::SlotVariantShapeView<'s>> {
    let fold = |text: &str| text.replace('_', "").to_ascii_lowercase();
    (0..64)
        .map_while(|index| shape.enum_variant(index))
        .find(|variant| fold(variant.name_str()) == fold(wanted))
}

#[cfg(test)]
mod tests {
    use lpc_model::{NodeDef, NodeKind, SlotAccess, SlotEditOp};
    use serde_json::json;

    use super::*;

    fn edits_for(kind: NodeKind, path: &str, value: Value) -> Result<Vec<String>, String> {
        let registry = SlotShapeRegistry::default();
        let root = registry
            .get_shape(NodeDef::default_for_kind(kind).shape_id())
            .expect("root shape");
        let path = SlotPath::parse(path).expect("path");
        json_slot_edits(&registry, root, &path, &value).map(|edits| {
            edits
                .into_iter()
                .map(|edit| match edit.op {
                    SlotEditOp::EnsurePresent => format!("ensure {}", edit.path),
                    SlotEditOp::AssignValue(value) => format!("assign {} {value:?}", edit.path),
                    SlotEditOp::Remove => format!("remove {}", edit.path),
                })
                .collect()
        })
    }

    #[test]
    fn a_map_value_ensures_each_entry_then_writes_its_leaves() {
        let edits = edits_for(
            NodeKind::Output,
            "ports",
            json!({ "0": { "endpoint": "ws281x:local:D6" } }),
        )
        .expect("ports");
        assert_eq!(edits[0], "ensure ports[0]");
        assert!(
            edits
                .iter()
                .any(|edit| edit.starts_with("assign ports[0].endpoint")),
            "{edits:?}"
        );
    }

    #[test]
    fn a_binding_entry_rides_its_endpoint_in_one_list() {
        // An empty binding is invalid on its own; the walk must produce the
        // entry AND its source together (they go out as one batch).
        let edits = edits_for(
            NodeKind::Playlist,
            "bindings",
            json!({ "time": { "source": "bus:time" } }),
        )
        .expect("bindings");
        assert_eq!(
            edits.first().map(String::as_str),
            Some("ensure bindings[time]")
        );
        assert!(
            edits
                .iter()
                .any(|edit| edit.starts_with("assign bindings[time].source.some")),
            "{edits:?}"
        );
    }

    #[test]
    fn an_option_is_made_present_and_its_value_set_through_some() {
        let edits = edits_for(
            NodeKind::Playlist,
            "cycle",
            json!({ "kind": "cycle", "step_seconds": 30.0, "fade_seconds": 1.5 }),
        )
        .expect("cycle");
        assert_eq!(edits[0], "ensure cycle");
        assert!(edits[1].starts_with("assign cycle.some"), "{edits:?}");
    }

    #[test]
    fn mismatches_and_unknown_paths_are_readable_rejections() {
        let reason = edits_for(NodeKind::Fixture, "render_size", json!("wide")).expect_err("type");
        assert!(reason.contains("an object {width"), "{reason}");
        let reason =
            edits_for(NodeKind::Fixture, "render_size.width", json!(300)).expect_err("inside");
        assert!(reason.contains("set `render_size` whole"), "{reason}");
        let reason = edits_for(NodeKind::Fixture, "no_such", json!(1)).expect_err("unknown");
        assert!(reason.contains("not a field"), "{reason}");
    }
}

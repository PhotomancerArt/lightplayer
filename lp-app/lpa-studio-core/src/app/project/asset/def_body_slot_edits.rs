//! A node file's pending whole-file body, said as slot edits.
//!
//! The board's overlay holds, per file, either slot edits or a whole body,
//! never both (`ArtifactOverlay::put_slot_edit` turns a body entry into a
//! slot one). Studio writes some node files whole (the Graph names an
//! output, sets a fixture's colour order), and some slots of the same file
//! one at a time (a Debug slot's `highlight` or `selection`, a value the
//! slot editor sets). So when a slot edit must land on a file whose body
//! is pending, Studio first says the body as the slot edits that turn the
//! file on disk into it ([`slot_edits_for_body`]); the board then holds
//! both, and Save writes what the body said
//! (docs/defects/2026-10-28-the-graph-names-an-output-the-board-keeps-it-unnamed.md).
//! [`materialize_def_text`] is the other direction: the file a set of slot
//! edits makes, as the board's commit would write it.

use lpc_model::slot_codec::snapshot_custom_slot_data;
use lpc_model::{
    NodeDef, Revision, SlotDataAccess, SlotEdit, SlotMapKey, SlotName, SlotOverlay, SlotPath,
    SlotShapeRegistry, lookup_slot_data_and_shape,
};
use lpc_registry::{
    ParseCtx, apply_op_to_def, apply_slot_overlay_to_def, parse_def_bytes, serialize_slot_draft,
};

/// The slot edits that make the node file `base` (its bytes on disk) read
/// as `body`: values assigned, map entries added and removed, variants
/// and options chosen. Checked: the edits applied to `base` must write
/// the same file `body` writes, else an error says where they part (a
/// body that changes the node's kind, a map whose order moved).
pub(crate) fn slot_edits_for_body(
    base: &[u8],
    body: &[u8],
    shapes: &SlotShapeRegistry,
) -> Result<Vec<SlotEdit>, String> {
    let ctx = ParseCtx { shapes };
    let mut scratch = parse_def_bytes(base, &ctx).map_err(|e| format!("the file on disk: {e}"))?;
    let target = parse_def_bytes(body, &ctx).map_err(|e| format!("the body: {e}"))?;
    if scratch.variant_name() != target.variant_name() {
        return Err(format!(
            "the body makes a {} a {}",
            scratch.variant_name(),
            target.variant_name()
        ));
    }
    let mut edits = Vec::new();
    diff(&mut scratch, &target, &ctx, &SlotPath::root(), &mut edits)?;
    let made = serialize_slot_draft(&scratch, &ctx).map_err(|e| e.to_string())?;
    let wanted = serialize_slot_draft(&target, &ctx).map_err(|e| e.to_string())?;
    if made != wanted {
        return Err("the slot edits don't write what the body says".to_string());
    }
    Ok(edits)
}

/// The node file `base` with `overlay`'s slot edits applied, written as the
/// board's commit writes it (Debug slots, which no file holds, left out).
pub(crate) fn materialize_def_text(
    base: &[u8],
    overlay: &SlotOverlay,
    shapes: &SlotShapeRegistry,
) -> Result<Vec<u8>, String> {
    let ctx = ParseCtx { shapes };
    let mut def = parse_def_bytes(base, &ctx).map_err(|e| e.to_string())?;
    apply_slot_overlay_to_def(&mut def, overlay, &ctx, FRAME).map_err(|e| e.to_string())?;
    serialize_slot_draft(&def, &ctx).map_err(|e| e.to_string())
}

/// A node file's bytes as the board's commit would write them.
pub(crate) fn canonical_def_text(
    bytes: &[u8],
    shapes: &SlotShapeRegistry,
) -> Result<Vec<u8>, String> {
    materialize_def_text(bytes, &SlotOverlay::new(), shapes)
}

/// The revision stamped on scratch edits (nothing reads it).
const FRAME: Revision = Revision(0);

/// Emit, under `path`, the edits that make `scratch` read as `target`,
/// applying each to `scratch` as it goes so a deeper look sees what the
/// board's overlay will make (a variant chosen takes its defaults, a map
/// entry added is there).
fn diff(
    scratch: &mut NodeDef,
    target: &NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    edits: &mut Vec<SlotEdit>,
) -> Result<(), String> {
    let wanted = facts_at(target, ctx, path)?;
    let held = facts_at(scratch, ctx, path)?;
    match (wanted, held) {
        (Facts::Value(wanted), Facts::Value(held)) => {
            if wanted != held {
                emit(
                    scratch,
                    ctx,
                    SlotEdit::assign_value(path.clone(), wanted),
                    edits,
                )?;
            }
            Ok(())
        }
        (Facts::Unit, Facts::Unit) => Ok(()),
        (Facts::Record(fields), Facts::Record(_)) => {
            for field in fields {
                diff(scratch, target, ctx, &path.child(field), edits)?;
            }
            Ok(())
        }
        (Facts::Map(wanted), Facts::Map(held)) => {
            for key in &held {
                if !wanted.contains(key) {
                    emit(
                        scratch,
                        ctx,
                        SlotEdit::remove(path.child_key(key.clone())),
                        edits,
                    )?;
                }
            }
            for key in wanted {
                if !held.contains(&key) {
                    emit(
                        scratch,
                        ctx,
                        SlotEdit::ensure_present(path.child_key(key.clone())),
                        edits,
                    )?;
                }
                diff(scratch, target, ctx, &path.child_key(key), edits)?;
            }
            Ok(())
        }
        (Facts::Enum(wanted), Facts::Enum(held)) => {
            if wanted != held {
                emit(
                    scratch,
                    ctx,
                    SlotEdit::ensure_present(path.child(wanted.clone())),
                    edits,
                )?;
            }
            diff(scratch, target, ctx, &path.child(wanted), edits)
        }
        (Facts::Option(wanted), Facts::Option(held)) => {
            let some = SlotName::parse("some").expect("'some' is a slot name");
            match (wanted, held) {
                (true, held) => {
                    if !held {
                        emit(
                            scratch,
                            ctx,
                            SlotEdit::ensure_present(path.child(some.clone())),
                            edits,
                        )?;
                    }
                    diff(scratch, target, ctx, &path.child(some), edits)
                }
                (false, true) => emit(scratch, ctx, SlotEdit::remove(path.child(some)), edits),
                (false, false) => Ok(()),
            }
        }
        (wanted, held) => Err(format!(
            "at {path} the body has a {} where the file has a {}",
            wanted.kind(),
            held.kind()
        )),
    }
}

/// Emit one edit and apply it to the scratch definition.
fn emit(
    scratch: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    edit: SlotEdit,
    edits: &mut Vec<SlotEdit>,
) -> Result<(), String> {
    apply_op_to_def(scratch, &edit, ctx, FRAME).map_err(|e| format!("{}: {e}", edit.path))?;
    edits.push(edit);
    Ok(())
}

/// What a slot holds, owned, so the scratch definition can change between
/// looks.
enum Facts {
    Value(lpc_model::LpValue),
    Record(Vec<SlotName>),
    Map(Vec<SlotMapKey>),
    Enum(SlotName),
    Option(bool),
    Unit,
}

impl Facts {
    fn kind(&self) -> &'static str {
        match self {
            Self::Value(_) => "value",
            Self::Record(_) => "record",
            Self::Map(_) => "map",
            Self::Enum(_) => "choice",
            Self::Option(_) => "option",
            Self::Unit => "unit",
        }
    }
}

/// The slot at `path` in `def`, a custom codec's slot (an asset path, say)
/// read as its plain data.
fn facts_at(def: &NodeDef, ctx: &ParseCtx<'_>, path: &SlotPath) -> Result<Facts, String> {
    let (data, shape) =
        lookup_slot_data_and_shape(def, ctx.shapes, path).map_err(|e| format!("{path}: {e}"))?;
    let data = match data {
        SlotDataAccess::Custom(custom) => {
            snapshot_custom_slot_data(custom.custom_codec_id(), custom)?
        }
        data => data,
    };
    Ok(match data {
        SlotDataAccess::Value(value) => Facts::Value(value.value()),
        SlotDataAccess::Unit(_) => Facts::Unit,
        SlotDataAccess::Record(_) => {
            let len = shape
                .record_fields_len()
                .ok_or_else(|| format!("a record with no record shape at {path}"))?;
            let mut fields = Vec::with_capacity(len);
            for index in 0..len {
                let field = shape.record_field(index).expect("index within field count");
                fields.push(
                    SlotName::parse(field.name_str())
                        .map_err(|e| format!("a field name at {path}: {e}"))?,
                );
            }
            Facts::Record(fields)
        }
        SlotDataAccess::Map(map) => Facts::Map(map.keys()),
        SlotDataAccess::Enum(choice) => Facts::Enum(
            SlotName::parse(choice.variant()).map_err(|e| format!("a variant at {path}: {e}"))?,
        ),
        SlotDataAccess::Option(option) => Facts::Option(option.data().is_some()),
        SlotDataAccess::Custom(custom) => {
            return Err(format!(
                "the custom slot {} at {path} has no plain data",
                custom.custom_codec_id()
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = r#"{
  "kind": "Output",
  "ports": {
    "0": { "endpoint": "ws281x:local:D0", "count": 64 },
    "1": { "endpoint": "ws281x:local:D1", "count": 64 }
  },
  "bindings": { "input": { "source": "bus:control.out" } }
}"#;

    fn shapes() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn canonical(text: &str) -> Vec<u8> {
        canonical_def_text(text.as_bytes(), &shapes()).expect("reads")
    }

    fn applied(base: &str, edits: &[SlotEdit]) -> Vec<u8> {
        let mut overlay = SlotOverlay::new();
        for edit in edits {
            overlay.put_edit(edit.clone());
        }
        materialize_def_text(base.as_bytes(), &overlay, &shapes()).expect("applies")
    }

    #[test]
    fn a_named_output_is_one_assignment() {
        let body = OUTPUT.replace(
            "\"kind\": \"Output\",",
            "\"kind\": \"Output\",\n  \"name\": \"1\",",
        );
        let edits = slot_edits_for_body(OUTPUT.as_bytes(), body.as_bytes(), &shapes()).unwrap();
        assert!(!edits.is_empty());
        assert_eq!(applied(OUTPUT, &edits), canonical(&body));
    }

    #[test]
    fn a_port_added_counted_and_removed_lands_through_the_overlay() {
        let body = OUTPUT
            .replace("\"count\": 64 },\n    \"1\"", "\"count\": 50 },\n    \"2\"")
            .replace("ws281x:local:D1", "ws281x:local:D2");
        let edits = slot_edits_for_body(OUTPUT.as_bytes(), body.as_bytes(), &shapes()).unwrap();
        assert_eq!(applied(OUTPUT, &edits), canonical(&body));
    }

    #[test]
    fn the_same_file_needs_no_edits() {
        let edits = slot_edits_for_body(OUTPUT.as_bytes(), OUTPUT.as_bytes(), &shapes()).unwrap();
        assert!(edits.is_empty());
    }

    #[test]
    fn a_body_of_another_kind_is_refused() {
        let clock = r#"{ "kind": "Clock" }"#;
        let err = slot_edits_for_body(OUTPUT.as_bytes(), clock.as_bytes(), &shapes()).unwrap_err();
        assert!(err.contains("Clock"), "{err}");
    }
}

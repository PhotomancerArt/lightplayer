//! Materialize a `MoveSlotEntry` mutation into per-path overlay edits.
//!
//! Keys are path segments, so changing a map entry's key is not a value edit:
//! the move is synthesized into the ordinary slot-edit vocabulary — an
//! `EnsurePresent` at the target entry, leaf assignments and structural
//! selections for every place the moved (effective) value diverges from the
//! entry the target would otherwise materialize as, and a `Remove` at the
//! source entry. The registry then feeds each synthesized edit through the
//! same base-relative normalization ordinary edits take.
//!
//! Divergences are computed against a **simulated future target entry**, not
//! against static factory defaults: the scratch definition is the base def
//! with the artifact's current overlay applied plus the *stored form* of the
//! leading `EnsurePresent` (which normalizes to removing a pending `Remove`
//! when the target key is base-present). That way the fresh entry is exactly
//! what the post-move derivation will produce — a factory default for a
//! base-absent key, the base entry for a base-present one.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::slot_codec::snapshot_custom_slot_data;
use lpc_model::{
    LpValue, NodeDef, Revision, SlotDataAccess, SlotEdit, SlotMapKey, SlotName, SlotOverlay,
    SlotPath, SlotPathSegment, lookup_slot_data_and_shape,
};

use crate::ParseCtx;

use super::apply_slot::{apply_op_to_def, apply_slot_overlay_to_def};

/// Synthesize the per-path edits a `MoveSlotEntry` materializes into.
///
/// `base` is the parsed base (unoverlaid) definition, `overlay` the
/// artifact's current pending slot edits, and `to_ensure_normalizes` whether
/// the leading `EnsurePresent` at `to` is a no-op against base (base-present
/// target key) and will therefore be stored as a removal of the overlay
/// entry at `to`. `from`/`to` are the entry paths as sent (they may carry a
/// root-variant prefix, which is resolved against the effective variant).
///
/// The returned edits are ordered parent-first: `EnsurePresent to`, the
/// divergence edits, `Remove from`. Callers apply base-relative
/// normalization per edit before storing.
pub(crate) fn synthesize_move_edits(
    base: NodeDef,
    overlay: &SlotOverlay,
    to_ensure_normalizes: bool,
    ctx: &ParseCtx<'_>,
    frame: Revision,
    from: &SlotPath,
    to: &SlotPath,
) -> Result<Vec<SlotEdit>, String> {
    // Scratch = the future effective def as far as the target entry is
    // concerned: base + current overlay + the stored form of the leading
    // ensure. The source entry's subtree is untouched by any of the to-side
    // edits, so the same scratch serves as the moved value's source too.
    let mut scratch_overlay = overlay.clone();
    if to_ensure_normalizes {
        scratch_overlay.remove_edit(to);
    } else {
        scratch_overlay.put_edit(SlotEdit::ensure_present(to.clone()));
    }
    let mut scratch = base;
    apply_slot_overlay_to_def(&mut scratch, &scratch_overlay, ctx, frame)
        .map_err(|error| error.to_string())?;

    let from_lookup = strip_root_variant_prefix(&scratch, from)?;
    let to_lookup = strip_root_variant_prefix(&scratch, to)?;

    let mut edits = Vec::new();
    edits.push(SlotEdit::ensure_present(to.clone()));
    diff_moved_value(
        &mut scratch,
        ctx,
        frame,
        &from_lookup,
        &to_lookup,
        to,
        &mut edits,
    )?;
    edits.push(SlotEdit::remove(from.clone()));
    Ok(edits)
}

/// Owned per-node facts extracted from a slot lookup, so the scratch def can
/// be mutated between inspections.
enum SlotFacts {
    Value(LpValue),
    Record(Vec<SlotName>),
    Map(Vec<SlotMapKey>),
    Enum(SlotName),
    Option(bool),
    Unit,
}

impl SlotFacts {
    fn kind(&self) -> &'static str {
        match self {
            Self::Value(_) => "value",
            Self::Record(_) => "record",
            Self::Map(_) => "map",
            Self::Enum(_) => "enum",
            Self::Option(_) => "option",
            Self::Unit => "unit",
        }
    }
}

/// Recursively emit edits under `to_emit` wherever the moved value at `from`
/// diverges from the fresh target value at `to` in `scratch`.
///
/// Structural edits are applied to `scratch` as they are emitted so deeper
/// lookups see exactly the state the real overlay application will produce
/// (a variant switch replaces the payload with that variant's defaults, a
/// map add creates the entry, and so on). `from`/`to` are lookup paths
/// (variant prefix stripped); `to_emit` preserves the original target path
/// for the emitted edits.
fn diff_moved_value(
    scratch: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    frame: Revision,
    from: &SlotPath,
    to: &SlotPath,
    to_emit: &SlotPath,
    edits: &mut Vec<SlotEdit>,
) -> Result<(), String> {
    let moved = facts_at(scratch, ctx, from)?;
    let fresh = facts_at(scratch, ctx, to)?;
    match (moved, fresh) {
        (SlotFacts::Value(moved), SlotFacts::Value(fresh)) => {
            if moved != fresh {
                edits.push(SlotEdit::assign_value(to_emit.clone(), moved));
            }
            Ok(())
        }
        (SlotFacts::Unit, SlotFacts::Unit) => Ok(()),
        (SlotFacts::Record(fields), SlotFacts::Record(_)) => {
            for field in fields {
                diff_moved_value(
                    scratch,
                    ctx,
                    frame,
                    &from.child(field.clone()),
                    &to.child(field.clone()),
                    &to_emit.child(field),
                    edits,
                )?;
            }
            Ok(())
        }
        (SlotFacts::Map(moved_keys), SlotFacts::Map(fresh_keys)) => {
            for key in &moved_keys {
                if !fresh_keys.contains(key) {
                    emit_and_apply(
                        scratch,
                        ctx,
                        frame,
                        SlotEdit::ensure_present(to_emit.child_key(key.clone())),
                        edits,
                    )?;
                }
                diff_moved_value(
                    scratch,
                    ctx,
                    frame,
                    &from.child_key(key.clone()),
                    &to.child_key(key.clone()),
                    &to_emit.child_key(key.clone()),
                    edits,
                )?;
            }
            for key in fresh_keys {
                if !moved_keys.contains(&key) {
                    emit_and_apply(
                        scratch,
                        ctx,
                        frame,
                        SlotEdit::remove(to_emit.child_key(key)),
                        edits,
                    )?;
                }
            }
            Ok(())
        }
        (SlotFacts::Enum(moved_variant), SlotFacts::Enum(fresh_variant)) => {
            if moved_variant != fresh_variant {
                emit_and_apply(
                    scratch,
                    ctx,
                    frame,
                    SlotEdit::ensure_present(to_emit.child(moved_variant.clone())),
                    edits,
                )?;
            }
            diff_moved_value(
                scratch,
                ctx,
                frame,
                &from.child(moved_variant.clone()),
                &to.child(moved_variant.clone()),
                &to_emit.child(moved_variant),
                edits,
            )
        }
        (SlotFacts::Option(moved_some), SlotFacts::Option(fresh_some)) => {
            let some = || SlotName::parse("some").expect("'some' is a valid slot name");
            match (moved_some, fresh_some) {
                (true, false) => {
                    emit_and_apply(
                        scratch,
                        ctx,
                        frame,
                        SlotEdit::ensure_present(to_emit.child(some())),
                        edits,
                    )?;
                    diff_moved_value(
                        scratch,
                        ctx,
                        frame,
                        &from.child(some()),
                        &to.child(some()),
                        &to_emit.child(some()),
                        edits,
                    )
                }
                (true, true) => diff_moved_value(
                    scratch,
                    ctx,
                    frame,
                    &from.child(some()),
                    &to.child(some()),
                    &to_emit.child(some()),
                    edits,
                ),
                // The fresh entry defaults to Some but the moved value is
                // None: null the target's body out.
                (false, true) => emit_and_apply(
                    scratch,
                    ctx,
                    frame,
                    SlotEdit::remove(to_emit.child(some())),
                    edits,
                ),
                (false, false) => Ok(()),
            }
        }
        (moved, fresh) => Err(format!(
            "moved value and fresh target diverge in kind at {to_emit}: {} vs {}",
            moved.kind(),
            fresh.kind()
        )),
    }
}

/// Emit one structural edit and apply it to the scratch def so subsequent
/// lookups observe its effect.
fn emit_and_apply(
    scratch: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    frame: Revision,
    edit: SlotEdit,
    edits: &mut Vec<SlotEdit>,
) -> Result<(), String> {
    apply_op_to_def(scratch, &edit, ctx, frame).map_err(|error| error.to_string())?;
    edits.push(edit);
    Ok(())
}

/// Owned facts about the slot at `path` in `def`, projecting custom codec
/// slots (e.g. `AssetSlot`) onto their declared plain data.
fn facts_at(def: &NodeDef, ctx: &ParseCtx<'_>, path: &SlotPath) -> Result<SlotFacts, String> {
    let (data, shape) =
        lookup_slot_data_and_shape(def, ctx.shapes, path).map_err(|error| error.to_string())?;
    let data = match data {
        SlotDataAccess::Custom(custom) => {
            snapshot_custom_slot_data(custom.custom_codec_id(), custom)?
        }
        data => data,
    };
    Ok(match data {
        SlotDataAccess::Value(value) => SlotFacts::Value(value.value()),
        SlotDataAccess::Unit(_) => SlotFacts::Unit,
        SlotDataAccess::Record(_) => {
            let len = shape
                .record_fields_len()
                .ok_or_else(|| format!("record data without a record shape at {path}"))?;
            let mut fields = Vec::with_capacity(len);
            for index in 0..len {
                let field = shape.record_field(index).expect("index within field count");
                fields
                    .push(SlotName::parse(field.name_str()).map_err(|error| {
                        format!("invalid record field name at {path}: {error}")
                    })?);
            }
            SlotFacts::Record(fields)
        }
        SlotDataAccess::Map(map) => SlotFacts::Map(map.keys()),
        SlotDataAccess::Enum(en) => SlotFacts::Enum(
            SlotName::parse(en.variant())
                .map_err(|error| format!("invalid enum variant name at {path}: {error}"))?,
        ),
        SlotDataAccess::Option(option) => SlotFacts::Option(option.data().is_some()),
        SlotDataAccess::Custom(custom) => {
            return Err(format!(
                "custom slot codec {} at {path} cannot be moved",
                custom.custom_codec_id()
            ));
        }
    })
}

/// Resolve a possible root-variant prefix against the effective definition:
/// a matching prefix strips, a mismatching one is an error (the path cannot
/// resolve), and bare paths pass through.
fn strip_root_variant_prefix(def: &NodeDef, path: &SlotPath) -> Result<SlotPath, String> {
    match path.segments().split_first() {
        Some((SlotPathSegment::Field(name), tail)) if NodeDef::is_variant_name(name.as_str()) => {
            if def.variant_name() == name.as_str() {
                Ok(SlotPath::from_segments(tail.to_vec()))
            } else {
                Err(format!(
                    "variant prefix {name} does not match the effective variant {}",
                    def.variant_name()
                ))
            }
        }
        _ => Ok(path.clone()),
    }
}

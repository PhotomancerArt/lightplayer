//! Apply slot-level artifact ops and serialize overlay drafts.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{
    NodeArtifact, NodeDef, Revision, SlotMutAccess, SlotName, SlotPath, SlotPathSegment,
    ensure_slot_present, remove_slot_map_entry, set_slot_option_none, set_slot_value,
    set_slot_variant_default,
};

use crate::edit_model::SlotEdit;
use crate::registry::ParseCtx;

use super::EditError;

pub fn serialize_slot_draft(def: &NodeDef, ctx: &ParseCtx<'_>) -> Result<Vec<u8>, EditError> {
    let text = NodeDef::write_toml(def, ctx.shapes).map_err(|err| EditError::Serialize {
        message: err.to_string(),
    })?;
    Ok(text.into_bytes())
}

/// Apply slot ops to an in-memory def (used by diff verification).
#[cfg(feature = "diff")]
pub(crate) fn apply_ops_to_node_def(
    def: &mut NodeDef,
    ops: &[SlotEdit],
    ctx: &ParseCtx<'_>,
    frame: Revision,
) -> Result<(), EditError> {
    for op in ops {
        apply_op_to_def(def, op, ctx, frame)?;
    }
    Ok(())
}

pub(crate) fn parse_def_bytes(bytes: &[u8], ctx: &ParseCtx<'_>) -> Result<NodeDef, EditError> {
    let text = core::str::from_utf8(bytes).map_err(|err| EditError::Parse {
        message: err.to_string(),
    })?;
    NodeDef::read_toml(ctx.shapes, text).map_err(|err| EditError::Parse {
        message: err.to_string(),
    })
}

pub(crate) fn apply_op_to_def(
    def: &mut NodeDef,
    op: &SlotEdit,
    ctx: &ParseCtx<'_>,
    frame: Revision,
) -> Result<(), EditError> {
    match op {
        SlotEdit::EnsurePresent { path } => apply_ensure_present(def, ctx, path, frame).map(drop),
        SlotEdit::AssignValue { path, value } => {
            let value_path = apply_ensure_present(def, ctx, path, frame)?;
            mutate_def(def, |root| {
                set_slot_value(root, ctx.shapes, &value_path, frame, value.clone())
            })
        }
        SlotEdit::Remove { path } => apply_remove(def, ctx, path, frame),
    }
}

fn apply_ensure_present(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    frame: Revision,
) -> Result<SlotPath, EditError> {
    if let Some((variant, tail)) = split_root_variant(path) {
        if def.variant_name() == variant.as_str() {
            mutate_def(def, |root| {
                ensure_slot_present(root, ctx.shapes, &tail, frame)
            })?;
            return Ok(tail);
        }

        let mut artifact = NodeArtifact::new(def.clone());
        if set_slot_variant_default(
            &mut artifact,
            ctx.shapes,
            &SlotPath::root(),
            frame,
            variant.as_str(),
        )
        .is_ok()
        {
            let mut switched = artifact.into_node_def();
            mutate_def(&mut switched, |root| {
                ensure_slot_present(root, ctx.shapes, &tail, frame)
            })?;
            *def = switched;
            return Ok(tail);
        }
    }
    mutate_def(def, |root| {
        ensure_slot_present(root, ctx.shapes, path, frame)
    })?;
    Ok(path.clone())
}

fn split_root_variant(path: &SlotPath) -> Option<(&SlotName, SlotPath)> {
    let (SlotPathSegment::Field(variant), tail) = path.segments().split_first()? else {
        return None;
    };
    if !NodeDef::is_variant_name(variant.as_str()) {
        return None;
    }
    Some((variant, SlotPath::from_segments(tail.to_vec())))
}

fn apply_remove(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    frame: Revision,
) -> Result<(), EditError> {
    let Some((parent, terminal)) = split_parent(path) else {
        return mutate_def(def, |root| {
            set_slot_option_none(root, ctx.shapes, path, frame)
        });
    };
    match terminal {
        SlotPathSegment::Key(key) => mutate_def(def, |root| {
            remove_slot_map_entry(root, ctx.shapes, &parent, frame, key)
        }),
        SlotPathSegment::Field(name) if name.as_str() == "some" => mutate_def(def, |root| {
            set_slot_option_none(root, ctx.shapes, &parent, frame)
        }),
        SlotPathSegment::Field(_) => mutate_def(def, |root| {
            set_slot_option_none(root, ctx.shapes, path, frame)
        }),
    }
}

fn split_parent(path: &SlotPath) -> Option<(SlotPath, &SlotPathSegment)> {
    let (terminal, parent) = path.segments().split_last()?;
    Some((SlotPath::from_segments(parent.to_vec()), terminal))
}

fn mutate_def(
    def: &mut NodeDef,
    f: impl FnOnce(&mut dyn SlotMutAccess) -> Result<(), lpc_model::SlotMutationError>,
) -> Result<(), EditError> {
    f(def).map_err(|err| EditError::SlotMutation {
        message: err.to_string(),
    })
}

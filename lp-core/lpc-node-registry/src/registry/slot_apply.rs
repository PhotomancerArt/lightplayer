//! Apply slot-level artifact ops and serialize overlay drafts.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{
    NodeArtifact, NodeDef, Revision, SlotMutAccess, SlotPath, SlotPathSegment, ensure_slot_present,
    remove_slot_map_entry, set_slot_option_none, set_slot_value, set_slot_variant_default,
};
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::edit::{AssetEdit, EditError, SlotEdit};

use super::{NodeDefRegistry, ParseCtx};

impl NodeDefRegistry {
    pub(crate) fn apply_slot_op(
        &mut self,
        path: LpPathBuf,
        op: &SlotEdit,
        _fs: &dyn LpFs,
        _ctx: &ParseCtx<'_>,
        _frame: Revision,
    ) -> Result<(), EditError> {
        ensure_toml_path(&path)?;
        let location = self.location_for_pending_path(LpPath::new(path.as_str()));
        if matches!(
            self.overlay.pending_at(&location).map(|p| &p.asset_edit),
            Some(AssetEdit::Delete)
        ) {
            return Err(EditError::InvalidPath {
                message: alloc::format!("artifact deleted pending commit: `{}`", path.as_str()),
            });
        }

        let pending = self.overlay.ensure_pending(location);
        pending.upsert_slot(op.clone());
        Ok(())
    }
}

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

fn ensure_toml_path(path: &LpPathBuf) -> Result<(), EditError> {
    if path.as_str().ends_with(".toml") {
        Ok(())
    } else {
        Err(EditError::InvalidPath {
            message: alloc::format!(
                "slot ops require a `.toml` artifact path, got `{}`",
                path.as_str()
            ),
        })
    }
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
        SlotEdit::EnsurePresent { path } => apply_ensure_present(def, ctx, path, frame),
        SlotEdit::AssignValue { path, value } => {
            apply_ensure_present(def, ctx, path, frame)?;
            mutate_def(def, |root| {
                set_slot_value(root, ctx.shapes, path, frame, value.clone())
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
) -> Result<(), EditError> {
    if let Some(SlotPathSegment::Field(variant)) = path.segments().first() {
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
            ensure_slot_present(&mut artifact, ctx.shapes, path, frame).map_err(|err| {
                EditError::SlotMutation {
                    message: err.to_string(),
                }
            })?;
            *def = artifact.into_node_def();
            return Ok(());
        }
    }
    mutate_def(def, |root| {
        ensure_slot_present(root, ctx.shapes, path, frame)
    })
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

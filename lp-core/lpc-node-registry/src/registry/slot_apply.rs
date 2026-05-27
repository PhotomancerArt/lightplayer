//! Apply slot-level artifact ops and serialize overlay drafts.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    LpValue, NodeArtifact, NodeDef, Revision, SlotMapKey, SlotMutAccess, SlotPath, SlotPathSegment,
    insert_slot_map_entry_default, lookup_slot_data_and_shape, remove_slot_map_entry,
    set_slot_option_none, set_slot_option_some_default, set_slot_value, set_slot_variant_default,
};
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::edit::{DefDraft, EditError, SlotEdit, SlotOverlayEntry};

use super::{NodeDefRegistry, ParseCtx};

impl NodeDefRegistry {
    pub(crate) fn apply_slot_op(
        &mut self,
        path: LpPathBuf,
        op: &SlotEdit,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        frame: Revision,
    ) -> Result<(), EditError> {
        ensure_toml_path(&path)?;
        if matches!(
            self.overlay.entry(LpPath::new(path.as_str())),
            Some(SlotOverlayEntry::Deleted)
        ) {
            return Err(EditError::InvalidPath {
                message: alloc::format!("artifact deleted pending commit: `{}`", path.as_str()),
            });
        }

        let mut def = self.fork_slot_draft(LpPath::new(path.as_str()), fs, ctx)?;
        apply_op_to_def(&mut def, op, ctx, frame)?;
        self.overlay.apply_def_draft(path, DefDraft::new(def));
        Ok(())
    }

    fn fork_slot_draft(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDef, EditError> {
        match self.overlay.entry(path) {
            Some(SlotOverlayEntry::DefDraft(draft)) => Ok(draft.def.clone()),
            Some(SlotOverlayEntry::Bytes(bytes)) => parse_def_bytes(bytes.as_slice(), ctx),
            Some(SlotOverlayEntry::Deleted) => Err(EditError::InvalidPath {
                message: alloc::format!("artifact deleted pending commit: `{}`", path.as_str()),
            }),
            None => self.fork_committed_def(path, fs, ctx),
        }
    }

    fn fork_committed_def(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDef, EditError> {
        let Some(location) = self.artifact_location_for_path(path) else {
            return Ok(NodeDef::default());
        };
        let bytes = self
            .read_committed_artifact_bytes(&location, fs)
            .map_err(|err| EditError::Parse {
                message: alloc::format!("read `{path:?}` for slot fork: {err:?}"),
            })?;
        parse_def_bytes(&bytes, ctx)
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

fn parse_def_bytes(bytes: &[u8], ctx: &ParseCtx<'_>) -> Result<NodeDef, EditError> {
    let text = core::str::from_utf8(bytes).map_err(|err| EditError::Parse {
        message: err.to_string(),
    })?;
    NodeDef::read_toml(ctx.shapes, text).map_err(|err| EditError::Parse {
        message: err.to_string(),
    })
}

fn apply_op_to_def(
    def: &mut NodeDef,
    op: &SlotEdit,
    ctx: &ParseCtx<'_>,
    frame: Revision,
) -> Result<(), EditError> {
    match op {
        SlotEdit::UseEnumVariant { path, variant } => {
            if path.is_root() {
                apply_root_use_enum_variant(def, ctx, frame, variant)
            } else {
                mutate_def(def, |root| {
                    set_slot_variant_default(root, ctx.shapes, path, frame, variant)
                })
            }
        }
        SlotEdit::AssignValue { path, value } => mutate_def(def, |root| {
            set_slot_value(root, ctx.shapes, path, frame, value.clone())
        }),
        SlotEdit::MapInsert { path, key, value } => {
            apply_map_insert(def, ctx, path, frame, key, value)
        }
        SlotEdit::MapRemove { path, key } => apply_map_remove(def, ctx, path, frame, key),
        SlotEdit::UseOption { path, present } => apply_use_option(def, ctx, path, frame, *present),
    }
}

fn apply_map_insert(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    frame: Revision,
    key: &str,
    value: &LpValue,
) -> Result<(), EditError> {
    let map_key = wire_map_key(key);
    mutate_def(def, |root| {
        insert_slot_map_entry_default(root, ctx.shapes, path, frame, &map_key)?;
        let value_path = if path.is_root() {
            SlotPath::from_segments(vec![SlotPathSegment::Key(map_key.clone())])
        } else {
            path.child_key(map_key)
        };
        if map_value_is_value_leaf(root, ctx, &value_path)
            .map_err(|err| lpc_model::SlotMutationError::unsupported_target(err.to_string()))?
        {
            set_slot_value(root, ctx.shapes, &value_path, frame, value.clone())?;
        }
        Ok(())
    })
}

fn map_value_is_value_leaf(
    root: &dyn SlotMutAccess,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
) -> Result<bool, EditError> {
    let (_, shape) = lookup_slot_data_and_shape(root, ctx.shapes, path).map_err(|err| {
        EditError::SlotMutation {
            message: err.to_string(),
        }
    })?;
    Ok(shape.value_shape().is_some())
}

fn apply_map_remove(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    frame: Revision,
    key: &str,
) -> Result<(), EditError> {
    let map_key = wire_map_key(key);
    mutate_def(def, |root| {
        remove_slot_map_entry(root, ctx.shapes, path, frame, &map_key)
    })
}

fn apply_use_option(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    frame: Revision,
    present: bool,
) -> Result<(), EditError> {
    if present {
        mutate_def(def, |root| {
            set_slot_option_some_default(root, ctx.shapes, path, frame)
        })
    } else {
        mutate_def(def, |root| {
            set_slot_option_none(root, ctx.shapes, path, frame)
        })
    }
}

fn apply_root_use_enum_variant(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    frame: Revision,
    variant: &str,
) -> Result<(), EditError> {
    let mut artifact = NodeArtifact::new(def.clone());
    set_slot_variant_default(&mut artifact, ctx.shapes, &SlotPath::root(), frame, variant)
        .map_err(|err| EditError::SlotMutation {
            message: err.to_string(),
        })?;
    *def = artifact.into_node_def();
    Ok(())
}

fn mutate_def(
    def: &mut NodeDef,
    f: impl FnOnce(&mut dyn SlotMutAccess) -> Result<(), lpc_model::SlotMutationError>,
) -> Result<(), EditError> {
    f(def).map_err(|err| EditError::SlotMutation {
        message: err.to_string(),
    })
}

fn wire_map_key(key: &str) -> SlotMapKey {
    if let Ok(value) = key.parse::<u32>() {
        SlotMapKey::U32(value)
    } else if let Ok(value) = key.parse::<i32>() {
        SlotMapKey::I32(value)
    } else {
        SlotMapKey::String(String::from(key))
    }
}

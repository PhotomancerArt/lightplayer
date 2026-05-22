//! Apply slot-level artifact ops and serialize overlay drafts.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactLocator, LpValue, NodeArtifact, NodeDef, NodeDefRef, NodeInvocation, Revision,
    SlotMapKey, SlotMutAccess, SlotPath, SlotPathSegment, insert_slot_map_entry_default,
    lookup_slot_data_and_shape, remove_slot_map_entry, set_slot_option_none,
    set_slot_option_some_default, set_slot_value, set_slot_variant_default,
};
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::edit::{DefDraft, EditError, EditOp, SlotOverlayEntry};
use crate::registry::def_walker::collect_invocations;

use super::{NodeDefRegistry, ParseCtx};

impl NodeDefRegistry {
    pub(crate) fn apply_slot_op(
        &mut self,
        path: LpPathBuf,
        op: &EditOp,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        frame: Revision,
    ) -> Result<(), EditError> {
        ensure_toml_path(&path)?;
        if matches!(
            self.slot_overlay.entry(LpPath::new(path.as_str())),
            Some(SlotOverlayEntry::Deleted)
        ) {
            return Err(EditError::InvalidPath {
                message: alloc::format!("artifact deleted pending commit: `{}`", path.as_str()),
            });
        }

        let mut def = self.fork_slot_draft(LpPath::new(path.as_str()), fs, ctx)?;
        apply_op_to_def(&mut def, op, ctx, frame)?;
        self.slot_overlay.apply_def_draft(path, DefDraft::new(def));
        Ok(())
    }

    fn fork_slot_draft(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDef, EditError> {
        match self.slot_overlay.entry(path) {
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
        let Some(artifact_id) = self.artifact_id_for_path(path) else {
            return Ok(NodeDef::default());
        };
        let bytes = self
            .read_committed_artifact_bytes(artifact_id, fs)
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
    ops: &[EditOp],
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
    op: &EditOp,
    ctx: &ParseCtx<'_>,
    frame: Revision,
) -> Result<(), EditError> {
    match op {
        EditOp::SetSlot { path, value } => apply_set_slot_on_def(def, ctx, path, frame, value),
        EditOp::MapInsert { path, key, value } => {
            apply_map_insert(def, ctx, path, frame, key, value)
        }
        EditOp::MapRemove { path, key } => apply_map_remove(def, ctx, path, frame, key),
        EditOp::OptionSet { path, present } => apply_option_set(def, ctx, path, frame, *present),
        EditOp::Delete | EditOp::SetBytes(_) => Err(EditError::UnsupportedOp { op: op.op_name() }),
    }
}

fn apply_set_slot_on_def(
    def: &mut NodeDef,
    ctx: &ParseCtx<'_>,
    path: &SlotPath,
    frame: Revision,
    value: &LpValue,
) -> Result<(), EditError> {
    if path.is_root() {
        if let LpValue::String(variant) = value {
            let mut artifact = NodeArtifact::new(def.clone());
            return mutate_def(&mut artifact, |root| {
                set_slot_variant_default(root, ctx.shapes, path, frame, variant)
            })
            .map(|()| {
                *def = artifact.into_node_def();
            });
        }
    } else if let LpValue::String(variant) = value {
        if mutate_def(def, |root| {
            set_slot_variant_default(root, ctx.shapes, path, frame, variant)
        })
        .is_ok()
        {
            return Ok(());
        }
    }
    if let Some((body, inner)) = inline_body_mutation(def, path) {
        return mutate_def(body, |root| {
            set_slot_value(root, ctx.shapes, &inner, frame, value.clone())
        });
    }
    if let Some(invocation) = project_node_def_mutation(def, path) {
        return apply_node_invocation_def(invocation, value);
    }
    mutate_def(def, |root| {
        set_slot_value(root, ctx.shapes, path, frame, value.clone())
    })
}

fn apply_node_invocation_def(
    invocation: &mut NodeInvocation,
    value: &LpValue,
) -> Result<(), EditError> {
    let LpValue::String(path) = value else {
        return Err(EditError::SlotMutation {
            message: String::from("node invocation def expects string path"),
        });
    };
    let locator = ArtifactLocator::parse(path).map_err(|err| EditError::SlotMutation {
        message: err.to_string(),
    })?;
    *invocation = NodeInvocation::path(locator);
    Ok(())
}

fn project_node_def_mutation<'a>(
    def: &'a mut NodeDef,
    path: &SlotPath,
) -> Option<&'a mut NodeInvocation> {
    let segs = path.segments();
    if segs.len() != 3 {
        return None;
    }
    let SlotPathSegment::Field(nodes) = &segs[0] else {
        return None;
    };
    if nodes.as_str() != "nodes" {
        return None;
    }
    let SlotPathSegment::Key(SlotMapKey::String(name)) = &segs[1] else {
        return None;
    };
    let SlotPathSegment::Field(def_field) = &segs[2] else {
        return None;
    };
    if def_field.as_str() != "def" {
        return None;
    }
    let NodeDef::Project(project) = def else {
        return None;
    };
    project.nodes.entries.get_mut(name.as_str())
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

fn apply_option_set(
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

fn inline_body_mutation<'a>(
    def: &'a mut NodeDef,
    path: &SlotPath,
) -> Option<(&'a mut NodeDef, SlotPath)> {
    let sites = collect_invocations(def, &SlotPath::root())
        .into_iter()
        .map(|site| site.path)
        .collect::<Vec<_>>();
    let (site_path, inner) = matching_inline_inner_path(path, &sites)?;
    let invocation = invocation_at_mut(def, &site_path)?;
    let NodeDefRef::Inline(body) = &mut invocation.def else {
        return None;
    };
    Some((body.as_mut(), inner))
}

fn matching_inline_inner_path(path: &SlotPath, sites: &[SlotPath]) -> Option<(SlotPath, SlotPath)> {
    for site_path in sites {
        let site_len = site_path.segments().len();
        let path_segs = path.segments();
        if path_segs.len() <= site_len {
            continue;
        }
        if path_segs[..site_len] != site_path.segments()[..site_len] {
            continue;
        }
        let SlotPathSegment::Field(name) = &path_segs[site_len] else {
            continue;
        };
        if name.as_str() != "def" {
            continue;
        }
        let inner = SlotPath::from_segments(path_segs[site_len + 1..].to_vec());
        return Some((site_path.clone(), inner));
    }
    None
}

fn invocation_at_mut<'a>(def: &'a mut NodeDef, path: &SlotPath) -> Option<&'a mut NodeInvocation> {
    let segs = path.segments();
    match def {
        NodeDef::Project(project) if segs.len() == 2 => {
            let SlotPathSegment::Field(nodes) = &segs[0] else {
                return None;
            };
            if nodes.as_str() != "nodes" {
                return None;
            }
            let SlotPathSegment::Key(SlotMapKey::String(name)) = &segs[1] else {
                return None;
            };
            project.nodes.entries.get_mut(name)
        }
        NodeDef::Playlist(playlist) if segs.len() == 3 => {
            let SlotPathSegment::Field(entries) = &segs[0] else {
                return None;
            };
            if entries.as_str() != "entries" {
                return None;
            }
            let SlotPathSegment::Key(key) = &segs[1] else {
                return None;
            };
            let SlotPathSegment::Field(node) = &segs[2] else {
                return None;
            };
            if node.as_str() != "node" {
                return None;
            }
            let key = match key {
                SlotMapKey::U32(value) => *value,
                SlotMapKey::I32(value) if *value >= 0 => *value as u32,
                _ => return None,
            };
            playlist
                .entries
                .entries
                .get_mut(&key)
                .map(|entry| &mut entry.node)
        }
        _ => None,
    }
}

fn mutate_def(
    root: &mut dyn SlotMutAccess,
    f: impl FnOnce(&mut dyn SlotMutAccess) -> Result<(), lpc_model::SlotMutationError>,
) -> Result<(), EditError> {
    f(root).map_err(|err| EditError::SlotMutation {
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

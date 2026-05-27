//! Fold committed artifact state with pending overlay edits.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{NodeDef, NodeDefParseError, Revision, current_revision};

use crate::edit::{ArtifactEdits, PendingAsset};

use super::effective_read::{def_state_at_source, parse_toml_bytes, read_error_state};
use super::slot_apply::{apply_op_to_def, parse_def_bytes, serialize_slot_draft};
use super::{NodeDefEntry, NodeDefLoc, NodeDefState, ParseCtx, RegistryError};

/// Effective raw bytes for an artifact (overlay ∪ committed).
pub fn project_artifact_bytes(
    committed: Option<&[u8]>,
    pending: Option<&ArtifactEdits>,
    ctx: &ParseCtx<'_>,
    frame: Revision,
) -> Result<Option<Vec<u8>>, RegistryError> {
    let Some(pending) = pending else {
        return Ok(committed.map(<[u8]>::to_vec));
    };

    match &pending.asset_edit {
        PendingAsset::Delete => return Ok(None),
        PendingAsset::ReplaceBody(bytes) => return Ok(Some(bytes.clone())),
        PendingAsset::None => {}
    }

    if pending.slot_edits_is_empty() {
        return Ok(committed.map(<[u8]>::to_vec));
    }

    let mut def = match committed {
        Some(bytes) => parse_def_bytes(bytes, ctx).map_err(edit_to_registry)?,
        None => NodeDef::default(),
    };

    for edit in pending.slot_edits() {
        apply_op_to_def(&mut def, edit, ctx, frame).map_err(edit_to_registry)?;
    }

    serialize_slot_draft(&def, ctx)
        .map(Some)
        .map_err(edit_to_registry)
}

/// Effective [`NodeDefState`] for an artifact root.
pub fn project_artifact_def(
    committed_state: &NodeDefState,
    pending: Option<&ArtifactEdits>,
    ctx: &ParseCtx<'_>,
) -> NodeDefState {
    let Some(pending) = pending else {
        return committed_state.clone();
    };

    match &pending.asset_edit {
        PendingAsset::Delete => {
            return NodeDefState::ParseError(read_error_state(crate::ArtifactError::Read(
                crate::ArtifactReadFailure::Deleted,
            )));
        }
        PendingAsset::ReplaceBody(bytes) => {
            return parse_toml_bytes(ctx, bytes);
        }
        PendingAsset::None => {}
    }

    if pending.slot_edits_is_empty() {
        return committed_state.clone();
    }

    let frame = current_revision();
    match committed_state {
        NodeDefState::Loaded(def) => {
            let mut projected = def.clone();
            for edit in pending.slot_edits() {
                if let Err(err) = apply_op_to_def(&mut projected, edit, ctx, frame) {
                    return NodeDefState::ParseError(NodeDefParseError::Toml {
                        error: err.to_string(),
                    });
                }
            }
            NodeDefState::Loaded(projected)
        }
        _ => match project_artifact_bytes(None, Some(pending), ctx, frame) {
            Ok(Some(bytes)) => parse_toml_bytes(ctx, &bytes),
            Ok(None) => NodeDefState::ParseError(read_error_state(crate::ArtifactError::Read(
                crate::ArtifactReadFailure::Deleted,
            ))),
            Err(err) => NodeDefState::ParseError(NodeDefParseError::Toml {
                error: alloc::format!("{err:?}"),
            }),
        },
    }
}

/// Effective state for a registered def location (inline slice of projected root).
pub fn project_def_at_loc(
    loc: &NodeDefLoc,
    root_entry: &NodeDefEntry,
    pending: Option<&ArtifactEdits>,
    ctx: &ParseCtx<'_>,
) -> NodeDefState {
    let root_state = project_artifact_def(&root_entry.state, pending, ctx);
    if loc.path.is_root() {
        return root_state;
    }

    match &root_state {
        NodeDefState::Loaded(root) => def_state_at_source(root, &loc.path).unwrap_or(root_state),
        other => other.clone(),
    }
}

fn edit_to_registry(err: crate::edit::EditError) -> RegistryError {
    RegistryError::InvalidPath {
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{LpValue, NodeDef, Revision, SlotPath, SlotShapeRegistry};

    fn ctx<'a>(shapes: &'a SlotShapeRegistry) -> ParseCtx<'a> {
        ParseCtx { shapes }
    }

    fn clock_def() -> NodeDef {
        NodeDef::from_toml_str(
            r#"
kind = "Clock"

[controls]
rate = 1.0
"#,
        )
        .expect("clock")
    }

    #[test]
    fn slot_pending_changes_effective_rate() {
        let shapes = SlotShapeRegistry::default();
        let parse_ctx = ctx(&shapes);
        let committed = serialize_slot_draft(&clock_def(), &parse_ctx).unwrap();
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(crate::edit::SlotEdit::AssignValue {
            path: SlotPath::parse("controls.rate").unwrap(),
            value: LpValue::F32(2.0),
        });

        let bytes = project_artifact_bytes(
            Some(&committed),
            Some(&pending),
            &parse_ctx,
            Revision::new(1),
        )
        .unwrap()
        .unwrap();
        let text = core::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("rate = 2"));
    }

    #[test]
    fn asset_replace_body() {
        let shapes = SlotShapeRegistry::default();
        let parse_ctx = ctx(&shapes);
        let body = b"void main() {}".to_vec();
        let mut pending = ArtifactEdits::default();
        pending.set_asset(PendingAsset::ReplaceBody(body.clone()));

        let bytes = project_artifact_bytes(None, Some(&pending), &parse_ctx, Revision::new(1))
            .unwrap()
            .unwrap();
        assert_eq!(bytes, body);
    }

    #[test]
    fn asset_delete_returns_none() {
        let shapes = SlotShapeRegistry::default();
        let parse_ctx = ctx(&shapes);
        let mut pending = ArtifactEdits::default();
        pending.set_asset(PendingAsset::Delete);

        let bytes =
            project_artifact_bytes(Some(b"x"), Some(&pending), &parse_ctx, Revision::new(1))
                .unwrap();
        assert!(bytes.is_none());
    }

    #[test]
    fn inline_child_projection() {
        let shapes = SlotShapeRegistry::default();
        let parse_ctx = ctx(&shapes);
        let root = NodeDef::from_toml_str(
            r#"
kind = "Playlist"

[entries.0.node.def]
kind = "Clock"

[entries.0.node.def.controls]
rate = 1.0
"#,
        )
        .expect("playlist");
        let committed = NodeDefState::Loaded(root);
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(crate::edit::SlotEdit::AssignValue {
            path: SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            value: LpValue::F32(3.0),
        });

        let loc = NodeDefLoc::artifact_root(crate::ArtifactLoc::file("/playlist.toml"));
        let entry = NodeDefEntry {
            loc: loc.clone(),
            state: committed,
            revision: Revision::new(1),
        };
        let effective = project_def_at_loc(
            &NodeDefLoc {
                path: SlotPath::parse("entries[0].node").unwrap(),
                ..loc
            },
            &entry,
            Some(&pending),
            &parse_ctx,
        );
        let NodeDefState::Loaded(NodeDef::Clock(def)) = effective else {
            panic!("expected clock child");
        };
        assert_eq!(*def.controls.rate.value(), 3.0);
    }
}

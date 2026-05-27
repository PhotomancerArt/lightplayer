//! Registry-shaped effective state projection from pending edits.

use alloc::string::ToString;

use lpc_model::{NodeDef, NodeDefParseError, NodeInvocation, SlotPath, current_revision};

use crate::edit_apply::{apply_op_to_def, project_artifact_bytes};
use crate::edit_model::{ArtifactEdits, AssetEdit};

use super::{NodeDefEntry, NodeDefLoc, NodeDefState, ParseCtx, RegistryError};

/// Effective [`NodeDefState`] for an artifact root.
pub(crate) fn project_artifact_def(
    committed_state: &NodeDefState,
    pending: Option<&ArtifactEdits>,
    ctx: &ParseCtx<'_>,
) -> NodeDefState {
    let Some(pending) = pending else {
        return committed_state.clone();
    };

    match &pending.asset_edit {
        AssetEdit::Delete => {
            return NodeDefState::ParseError(read_error_state(crate::ArtifactError::Read(
                crate::ArtifactReadFailure::Deleted,
            )));
        }
        AssetEdit::ReplaceBody(bytes) => {
            return parse_toml_bytes(ctx, bytes);
        }
        AssetEdit::None => {}
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
pub(crate) fn project_def_at_loc(
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
        NodeDefState::Loaded(root) => def_state_at_path(root, &loc.path).unwrap_or(root_state),
        other => other.clone(),
    }
}

pub(crate) fn parse_toml_bytes(ctx: &ParseCtx<'_>, bytes: &[u8]) -> NodeDefState {
    let text = match core::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            return NodeDefState::ParseError(NodeDefParseError::Toml {
                error: err.to_string(),
            });
        }
    };
    match NodeDef::read_toml(ctx.shapes, text) {
        Ok(def) => NodeDefState::Loaded(def),
        Err(err) => NodeDefState::ParseError(err),
    }
}

pub(crate) fn read_error_state(err: crate::ArtifactError) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: alloc::format!("artifact read failed: {err:?}"),
    }
}

pub(crate) fn edit_to_registry(err: crate::edit_apply::EditError) -> RegistryError {
    RegistryError::InvalidPath {
        message: err.to_string(),
    }
}

fn def_state_at_path(root: &NodeDef, path: &SlotPath) -> Option<NodeDefState> {
    if path.is_root() {
        return Some(NodeDefState::Loaded(root.clone()));
    }
    for site in root.invocation_sites(&SlotPath::root()) {
        if site.path == *path {
            return match &site.invocation {
                NodeInvocation::Unset | NodeInvocation::Ref(_) => None,
                NodeInvocation::Def(body) => Some(NodeDefState::Loaded(body.value().clone())),
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use lpc_model::{LpValue, NodeDef, Revision, SlotPath, SlotShapeRegistry};

    fn ctx<'a>(shapes: &'a SlotShapeRegistry) -> ParseCtx<'a> {
        ParseCtx { shapes }
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
        pending.upsert_slot(crate::edit_model::SlotEdit::AssignValue {
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

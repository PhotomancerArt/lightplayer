//! `diff(base, target) -> ProjectOverlay`.

use alloc::collections::BTreeSet;

use lpc_model::{ArtifactBodyEdit, NodeDef, ProjectOverlay};
use lpfs::LpPathBuf;

use crate::ParseCtx;

use super::DiffError;
use super::def_diff::diff_node_defs;
use super::snapshot::ProjectSnapshot;

/// Compute overlay pending state that transforms `base` into `target`.
pub fn diff(
    base: &ProjectSnapshot,
    target: &ProjectSnapshot,
    ctx: &ParseCtx<'_>,
) -> Result<ProjectOverlay, DiffError> {
    let mut paths = BTreeSet::new();
    paths.extend(base.paths());
    paths.extend(target.paths());

    let mut overlay = ProjectOverlay::new();
    for path in paths {
        let base_bytes = base.get(path);
        let target_bytes = target.get(path);
        match (base_bytes, target_bytes) {
            (None, None) => {}
            (Some(_), None) => {
                overlay.set_artifact_body(LpPathBuf::from(path), ArtifactBodyEdit::Delete);
            }
            (None, Some(bytes)) | (Some(_), Some(bytes)) if base_bytes != target_bytes => {
                if path.ends_with(".toml") {
                    let base_def = parse_toml_def(base_bytes, ctx, path)?;
                    let target_def = parse_toml_def(Some(bytes), ctx, path)?;
                    let ops = diff_node_defs(&base_def, &target_def, ctx)?;
                    if !ops.is_empty() {
                        for op in ops {
                            overlay.put_slot_edit(LpPathBuf::from(path), op);
                        }
                    }
                } else {
                    overlay.set_artifact_body(
                        LpPathBuf::from(path),
                        ArtifactBodyEdit::ReplaceBody(bytes.to_vec()),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(overlay)
}

fn parse_toml_def(
    bytes: Option<&[u8]>,
    ctx: &ParseCtx<'_>,
    path: &str,
) -> Result<NodeDef, DiffError> {
    let Some(bytes) = bytes else {
        return Ok(NodeDef::default());
    };
    let text = core::str::from_utf8(bytes).map_err(|err| DiffError::Parse {
        message: alloc::format!("`{path}` utf-8: {err}"),
    })?;
    NodeDef::read_toml(ctx.shapes, text).map_err(|err| DiffError::Parse {
        message: alloc::format!("`{path}`: {err}"),
    })
}

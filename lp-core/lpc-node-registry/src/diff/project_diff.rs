//! `diff(base, target) -> EditBatch`.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::NodeDef;
use lpfs::LpPathBuf;

use crate::ParseCtx;
use crate::edit::{ArtifactEdit, EditOp, EditTarget, EditBatch, EditBatchId};

use super::DiffError;
use super::def_diff::diff_node_defs;
use super::snapshot::ProjectSnapshot;

/// Compute a change set that transforms `base` into `target`.
pub fn diff(
    base: &ProjectSnapshot,
    target: &ProjectSnapshot,
    ctx: &ParseCtx<'_>,
) -> Result<EditBatch, DiffError> {
    let mut paths = BTreeSet::new();
    paths.extend(base.paths());
    paths.extend(target.paths());

    let mut changes = Vec::new();
    for path in paths {
        let base_bytes = base.get(path);
        let target_bytes = target.get(path);
        match (base_bytes, target_bytes) {
            (None, None) => {}
            (Some(_), None) => changes.push(ArtifactEdit {
                target: EditTarget::Path(LpPathBuf::from(path)),
                ops: vec![EditOp::Delete],
            }),
            (None, Some(bytes)) | (Some(_), Some(bytes)) if base_bytes != target_bytes => {
                if path.ends_with(".toml") {
                    let base_def = parse_toml_def(base_bytes, ctx, path)?;
                    let target_def = parse_toml_def(Some(bytes), ctx, path)?;
                    let ops = diff_node_defs(&base_def, &target_def, ctx)?;
                    if !ops.is_empty() {
                        changes.push(ArtifactEdit {
                            target: EditTarget::Path(LpPathBuf::from(path)),
                            ops,
                        });
                    }
                } else {
                    let text = core::str::from_utf8(bytes).map_err(|err| DiffError::Parse {
                        message: alloc::format!("`{path}` utf-8: {err}"),
                    })?;
                    changes.push(ArtifactEdit {
                        target: EditTarget::Path(LpPathBuf::from(path)),
                        ops: vec![EditOp::SetBytes(String::from(text))],
                    });
                }
            }
            _ => {}
        }
    }
    Ok(EditBatch::new(EditBatchId(0), changes))
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

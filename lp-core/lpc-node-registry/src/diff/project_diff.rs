//! `diff(base, target) -> OverlayDelta`.

use alloc::collections::BTreeSet;

use lpc_model::NodeDef;
use lpfs::LpPathBuf;

use crate::ParseCtx;
use crate::edit::{ArtifactEdits, OverlayDelta, PendingAsset};

use super::DiffError;
use super::def_diff::diff_node_defs;
use super::snapshot::ProjectSnapshot;

/// Compute overlay pending state that transforms `base` into `target`.
pub fn diff(
    base: &ProjectSnapshot,
    target: &ProjectSnapshot,
    ctx: &ParseCtx<'_>,
) -> Result<OverlayDelta, DiffError> {
    let mut paths = BTreeSet::new();
    paths.extend(base.paths());
    paths.extend(target.paths());

    let mut delta = OverlayDelta::new();
    for path in paths {
        let base_bytes = base.get(path);
        let target_bytes = target.get(path);
        match (base_bytes, target_bytes) {
            (None, None) => {}
            (Some(_), None) => {
                let mut pending = ArtifactEdits::default();
                pending.set_asset(PendingAsset::Delete);
                delta.insert(LpPathBuf::from(path), pending);
            }
            (None, Some(bytes)) | (Some(_), Some(bytes)) if base_bytes != target_bytes => {
                if path.ends_with(".toml") {
                    let base_def = parse_toml_def(base_bytes, ctx, path)?;
                    let target_def = parse_toml_def(Some(bytes), ctx, path)?;
                    let ops = diff_node_defs(&base_def, &target_def, ctx)?;
                    if !ops.is_empty() {
                        let mut pending = ArtifactEdits::default();
                        for op in ops {
                            pending.upsert_slot(op);
                        }
                        delta.insert(LpPathBuf::from(path), pending);
                    }
                } else {
                    let mut pending = ArtifactEdits::default();
                    pending.set_asset(PendingAsset::ReplaceBody(bytes.to_vec()));
                    delta.insert(LpPathBuf::from(path), pending);
                }
            }
            _ => {}
        }
    }
    Ok(delta)
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

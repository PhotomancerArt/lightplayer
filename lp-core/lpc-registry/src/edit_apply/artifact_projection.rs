//! Fold committed artifact bytes with pending overlay edits.

use alloc::vec::Vec;

use lpc_model::{ArtifactBodyEdit, ArtifactOverlay, NodeDef, Revision};

use super::{apply_op_to_def, parse_def_bytes, serialize_slot_draft};

use super::EditError;
use crate::ParseCtx;

/// Effective raw bytes for an artifact (overlay ∪ committed).
pub fn project_artifact_bytes(
    committed: Option<&[u8]>,
    pending: Option<&ArtifactOverlay>,
    ctx: &ParseCtx<'_>,
    frame: Revision,
) -> Result<Option<Vec<u8>>, EditError> {
    let Some(pending) = pending else {
        return Ok(committed.map(<[u8]>::to_vec));
    };

    let ArtifactOverlay::Slot { overlay } = pending else {
        return match pending.as_body() {
            Some(ArtifactBodyEdit::Delete) => Ok(None),
            Some(ArtifactBodyEdit::ReplaceBody(bytes)) => Ok(Some(bytes.clone())),
            None => Ok(committed.map(<[u8]>::to_vec)),
        };
    };

    if overlay.is_empty() {
        return Ok(committed.map(<[u8]>::to_vec));
    }
    let mut def = match committed {
        Some(bytes) => parse_def_bytes(bytes, ctx)?,
        None => NodeDef::default(),
    };

    for edit in overlay.to_apply_plan() {
        apply_op_to_def(&mut def, &edit, ctx, frame)?;
    }

    serialize_slot_draft(&def, ctx).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{LpValue, NodeDef, Revision, SlotEdit, SlotPath, SlotShapeRegistry};

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
        let mut pending = ArtifactOverlay::slot(lpc_model::SlotOverlay::new());
        pending.put_slot_edit(SlotEdit::assign_value(
            SlotPath::parse("controls.rate").unwrap(),
            LpValue::F32(2.0),
        ));

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
        let pending = ArtifactOverlay::body(ArtifactBodyEdit::ReplaceBody(body.clone()));

        let bytes = project_artifact_bytes(None, Some(&pending), &parse_ctx, Revision::new(1))
            .unwrap()
            .unwrap();
        assert_eq!(bytes, body);
    }

    #[test]
    fn asset_delete_returns_none() {
        let shapes = SlotShapeRegistry::default();
        let parse_ctx = ctx(&shapes);
        let pending = ArtifactOverlay::body(ArtifactBodyEdit::Delete);

        let bytes =
            project_artifact_bytes(Some(b"x"), Some(&pending), &parse_ctx, Revision::new(1))
                .unwrap();
        assert!(bytes.is_none());
    }
}

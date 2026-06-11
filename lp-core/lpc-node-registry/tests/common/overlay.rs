//! Shared overlay mutation helpers for integration tests.

use lpc_model::{ArtifactBodyEdit, Revision, SlotShapeRegistry};
use lpc_node_registry::{NodeDefRegistry, ParseCtx, SlotEdit};
use lpfs::{LpFs, LpPathBuf};

pub fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

pub fn upsert_slot(
    registry: &mut NodeDefRegistry,
    fs: &dyn LpFs,
    path: &str,
    op: SlotEdit,
    frame: Revision,
) {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .upsert_slot_edit(LpPathBuf::from(path), op, fs, &ctx, frame)
        .unwrap();
}

pub fn set_pending_artifact_body_bytes(registry: &mut NodeDefRegistry, path: &str, bytes: &[u8]) {
    registry
        .set_pending_artifact_body(
            LpPathBuf::from(path),
            ArtifactBodyEdit::ReplaceBody(bytes.to_vec()),
        )
        .unwrap();
}

pub fn set_pending_artifact_body_text(registry: &mut NodeDefRegistry, path: &str, text: &str) {
    set_pending_artifact_body_bytes(registry, path, text.as_bytes());
}

pub fn delete_pending_artifact_body(registry: &mut NodeDefRegistry, path: &str) {
    registry
        .set_pending_artifact_body(LpPathBuf::from(path), ArtifactBodyEdit::Delete)
        .unwrap();
}

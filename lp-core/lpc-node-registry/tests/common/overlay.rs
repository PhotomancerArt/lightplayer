//! Shared overlay mutation helpers for integration tests.

use lpc_model::{Revision, SlotShapeRegistry};
use lpc_node_registry::{NodeDefRegistry, ParseCtx, PendingAsset, SlotEdit};
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

pub fn set_pending_asset_bytes(registry: &mut NodeDefRegistry, path: &str, bytes: &[u8]) {
    registry
        .set_pending_asset(
            LpPathBuf::from(path),
            PendingAsset::ReplaceBody(bytes.to_vec()),
        )
        .unwrap();
}

pub fn set_pending_asset_text(registry: &mut NodeDefRegistry, path: &str, text: &str) {
    set_pending_asset_bytes(registry, path, text.as_bytes());
}

pub fn delete_pending_asset(registry: &mut NodeDefRegistry, path: &str) {
    registry
        .set_pending_asset(LpPathBuf::from(path), PendingAsset::Delete)
        .unwrap();
}

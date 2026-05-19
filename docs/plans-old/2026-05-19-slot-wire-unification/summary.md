# Slot Wire Unification Summary

Implemented a single slot sync serialization path for debug/client wire data.

## What changed

- Fixed paged shape sync cursor semantics so the next page starts after the last
  included shape id instead of skipping the first omitted shape.
- Added `lpc_model::slot_sync_codec`, a canonical sync snapshot codec that:
  - writes and reads slot snapshots through registered `SlotShape`s,
  - preserves value/container revisions,
  - preserves typed map keys without relying on JSON object field names,
  - rejects payloads that do not match the expected slot shape.
- Switched wire root snapshots and patch replacements to carry `WireSlotData`
  containing slot sync JSON.
- Switched allocated and streaming project-read node slot writers onto the same
  sync snapshot writer.
- Switched the client slot mirror to strict sync snapshot decode only. The old
  SlotCodec-first / SlotData-Serde fallback path is gone.
- Removed Serde derives, Serde attributes, and the Serde map-entry helper from
  `SlotData` and its dynamic container/key types.

## Validation

- `cargo fmt --check`
- `cargo check -p lpc-engine`
- `cargo check -p lpa-server`
- `cargo test -p lpa-server --no-run`
- `cargo test -p lpc-model slot_sync_codec`
- `cargo test -p lpc-model snapshot_page`
- `cargo test -p lpc-wire slot::sync::tests`
- `cargo test -p lpc-wire real_source_defs_sync_as_slot_roots`
- `cargo test -p lpc-view slot::mirror::tests`
- `cargo test -p lpc-engine project_read_stream::tests`
- `cargo test -p lp-cli paged_shape_sync_keeps_prior_pages_when_final_page_is_complete -- --nocapture`
- `rg` check confirmed no Serde bindings remain in `lp-core/lpc-model/src/slot/slot_data.rs`.

`cargo test -p lpc-slot-mockup storage_codec` was attempted but is currently
blocked by pre-existing mockup drift: `ShaderDef` no longer has
`consumed_slots` or `add_consumed_slot`.

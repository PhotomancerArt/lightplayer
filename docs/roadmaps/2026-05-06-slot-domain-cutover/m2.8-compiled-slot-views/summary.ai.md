# M2.8 Compiled Slot Views Summary

## What Was Built

- Added `SlotAccessor`, an indexed compiled form of `SlotPath` that is validated against `SlotShapeRegistry::revision()`.
- Added accessor-based consumed-slot resolution so authored defaults can be read through compiled index paths while bindings still match on semantic `SlotPath`.
- Extended `TickContext` with `resolve_consumed_slot_accessor_value`.
- Added build-time generation of root `*View` types from `#[slot(root, view)]` records.
- Generated `TextureDefView` in `lpc-model` alongside the static shape bootstrap output.
- Removed the engine-owned `TextureDefView` wrapper; texture nodes now cache the model-generated view directly.
- Cached the texture def view on `TextureNode` and rebuilt it when the registry revision changes.

## Decisions For Future Reference

### Registry Revision Invalidation

- **Decision:** Compiled accessors are invalidated by the registry-wide `ids_revision`.
- **Why:** This is conservative, simple, and safe while shape churn is rare.
- **Rejected alternatives:** Per-root revision dependency tracking.
- **Revisit when:** Dynamic shader param shapes churn often enough that global invalidation becomes noisy.

### Accessor Queries

- **Decision:** Add `QueryKey::ConsumedSlotAccessor` alongside the existing path-based consumed-slot query.
- **Why:** Binding matching still needs the semantic path, but authored default reads can now use indexed access.
- **Rejected alternatives:** Replace all resolver keys with accessors immediately.
- **Revisit when:** Produced slots and binding registries are ready for compact slot identity.

### Generated Views Stay Model-Side And Build-Time

- **Decision:** `lpc-model/build.rs` generates accessors-only `*View` structs into `OUT_DIR/slot_views.rs`.
- **Why:** Build-time generated Rust is visible as a real included file, avoids proc-macro sibling-type IDE edge cases, and keeps resolver-specific reads out of `lpc-model`.
- **Rejected alternatives:** Proc-macro sibling view types, hand-authored engine wrapper files, and generated engine-specific `TickContext` methods.
- **Revisit when:** Client-side or engine-side codegen gets its own crate.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-model`
- `cargo test -p lpc-model --features derive --test slot_record_derive`
- `cargo test -p lpc-engine texture`
- `cargo check -p lpc-model --features schema-gen`
- `cargo clippy -p lpc-engine -p lpc-model -p lpc-slot-codegen -p lpc-slot-macros --all-targets -- -D warnings`

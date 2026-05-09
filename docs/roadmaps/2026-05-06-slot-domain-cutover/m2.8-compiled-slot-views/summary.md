# M2.8 Compiled Slot Views Summary

## What Was Built

- Added `SlotAccessor`, an indexed compiled form of `SlotPath` that is validated against `SlotShapeRegistry::revision()`.
- Added accessor-based consumed-slot resolution so authored defaults can be read through compiled index paths while bindings still match on semantic `SlotPath`.
- Extended `TickContext` with `resolve_consumed_slot_accessor_value`.
- Extended `#[derive(SlotRecord)]` to generate root `*View` types containing compiled accessors.
- Converted the engine `TextureDefView` wrapper to use the generated `lpc_model::TextureDefView`.
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

### Generated Views Stay Model-Side

- **Decision:** The derive macro generates accessors-only `*View` structs in `lpc-model`.
- **Why:** `lpc-model` cannot depend on `lpc-engine`, so resolver-backed typed reads remain engine wrappers around generated accessors.
- **Rejected alternatives:** Generate engine-specific `TickContext` methods directly from the model derive.
- **Revisit when:** Client-side or engine-side codegen gets its own crate.

## Validation

- `cargo fmt --check`
- `cargo test -p lpc-model`
- `cargo test -p lpc-model --features derive --test slot_accessor --test slot_record_derive`
- `cargo test -p lpc-engine`
- `cargo clippy -p lpc-engine -p lpc-model -p lpc-slot-macros --all-targets -- -D warnings`


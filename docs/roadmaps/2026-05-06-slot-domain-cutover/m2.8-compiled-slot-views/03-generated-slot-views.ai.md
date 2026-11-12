# Phase 3: Generated Slot Views

## Scope

Extend the existing `lpc-slot-codegen` build step to generate simple read-only views for root records marked with `#[slot(root, view)]`.

Out of scope:

- Perfect generated API for maps, enums, options, and nested records.
- Writable views.
- Client-side view integration.

## Implementation Details

In `lpc-slot-codegen`, scan the model crate for `#[derive(SlotRecord)]` + `#[slot(root, view)]` records and emit a type into `OUT_DIR/slot_views.rs`:

```rust
pub struct <RecordName>View {
    // one SlotAccessor per readable value field
}
```

For each field that is a value leaf and whose Rust type has `FromLpValue`, generate:

- A compiled accessor field.
- A compile-time path based on the slot field name.
- An accessor method used by `TickContext` or client-side dynamic readers.

The first target is `TextureDefView`. Generated views are re-exported from `lpc-model`, so engine nodes do not need hand-authored wrapper files.

Add tests showing:

- A root record marked with `#[slot(root, view)]` generates a `*View`.
- The view compiles against a registry.
- Engine code resolves values through the generated accessors and `TickContext`.

## Validate

```bash
cargo test -p lpc-model --test slot_record_derive
cargo test -p lpc-engine
```

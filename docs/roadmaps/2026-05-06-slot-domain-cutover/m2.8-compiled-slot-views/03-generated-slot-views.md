# Phase 3: Generated Slot Views

## Scope

Extend `#[derive(SlotRecord)]` to generate simple read-only views for root records.

Out of scope:

- Perfect generated API for maps, enums, options, and nested records.
- Writable views.
- Client-side view integration.

## Implementation Details

In `lpc-slot-macros`, for `#[derive(SlotRecord)]` + `#[slot(root)]`, emit a sibling type:

```rust
pub struct <RecordName>View {
    // one SlotAccessor per readable value field
}
```

For each field that is a value leaf and whose Rust type has `FromLpValue`, generate:

- A compiled accessor field.
- A compile-time path based on the slot field name.
- A typed read method that delegates to `TickContext`.

The first target is `TextureDefView`. If name collision with the hand-authored file is awkward, either:

- Move generated-compatible logic into a separate `CompiledTextureDefView`, or
- Remove the manual `texture_def_view.rs` once macro output owns the type.

Add derive tests showing:

- A root record derives a `*View`.
- The view compiles against a registry.
- The view field method returns the expected value through a tiny fake context or compile-only API.

If the macro cannot depend on `lpc-engine` types, keep the generated view model in `lpc-model` as accessors-only and let engine-specific extension methods live in `lpc-engine`.

## Validate

```bash
cargo test -p lpc-model --test slot_record_derive
cargo test -p lpc-engine
```


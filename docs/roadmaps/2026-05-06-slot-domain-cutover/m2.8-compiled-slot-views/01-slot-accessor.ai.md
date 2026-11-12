# Phase 1: SlotAccessor

## Scope

Add the reusable compiled accessor model in `lpc-model`.

Out of scope:

- Mutating through accessors.
- Full optimizer-style caching.
- Removing `lookup_slot_data`.

## Implementation Details

Add `lp-core/lpc-model/src/slot/slot_accessor.rs`.

Create:

- `SlotAccessor`
- `SlotAccessorStep`
- `SlotAccessorError`

The first implementation should support:

- Compile from `root_shape_id`, `SlotPath`, and `SlotShapeRegistry`.
- Record field segments compile to field indices.
- `SlotShape::Ref` is followed while compiling and while checking.
- Access returns `SlotDataAccess` using index-based `SlotRecordAccess::field(index)`.
- Access rejects registry revision mismatch.

Expose the module from `slot/mod.rs` and re-export from `lib.rs`.

Add tests for:

- Compiling `output` on a simple record root.
- Accessing the value without string lookup.
- Missing field fails during compile.
- Registry revision mismatch fails during access.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model --test slot_accessor
```


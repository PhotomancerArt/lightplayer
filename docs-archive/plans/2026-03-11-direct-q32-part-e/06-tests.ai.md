# Phase 6: Tests & Validation

## Primary validation

`scripts/filetests.sh` — the full shader test suite. This exercises
the entire pipeline end-to-end with Q32 direct emission. Must pass
unchanged after all cleanup.

## What to watch for

- **Missing imports**: After deleting `backend/transform/`, any leftover
  reference will fail to compile. `cargo check` catches these.
- **`Q32Options` path**: The re-export in `lib.rs` must point to
  `backend::q32::Q32Options` (not the old transform path). External
  consumers (filetests, metrics app, etc.) use this re-export.
- **Dead code warnings**: After removing the transform, check for
  newly-unused code that was only needed by the transform (e.g.,
  `FixedPointFormat` methods that nothing else calls).
- **`lps-q32-metrics-app`**: Uses `Q32Options` — verify it still
  compiles after the path change.
- **`lps-builtins-gen-app`**: Was updated in Plan C to generate
  `mapping.rs` instead of `math.rs`. Verify it doesn't reference the
  deleted transform directory.

## Ordered/Unordered fix

Not currently exercised by the frontend, so no existing test will verify
the fix. Add a unit test in `numeric.rs` tests:

```rust
#[test]
fn q32_cmp_ordered_unordered() {
    // Ordered is always true (Q32 has no NaN)
    // Unordered is always false
    run_q32_cmp_test(FloatCC::Ordered, 0x10000, 0x20000, 1);
    run_q32_cmp_test(FloatCC::Unordered, 0x10000, 0x20000, 0);
}
```

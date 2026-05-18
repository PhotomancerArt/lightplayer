# Phase 6: Update Mockup And Codec Tests

## Scope Of Phase

Make the mockup use the new `ValueSlot<T: SlotValue>` model naturally.

In scope:

- Update mockup source/domain types to use semantic values and aliases.
- Remove mockup-specific hand-coded leaf slot machinery where generic `ValueSlot<T>` works.
- Keep codec tests focused on the generic slot model.
- Note any remaining deviations from the real domain.

Out of scope:

- Full adoption in real storage/message paths.
- Solving all enum/wrapper serialization issues.

## Code Organization Reminders

- Keep mockup types readable as examples of the intended model.
- Do not add hidden static field lists.
- Prefer generated/discovered shape metadata.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-core/lpc-slot-mockup/src/source/**`
- `lp-core/lpc-slot-mockup/src/engine/**`
- `lp-core/lpc-slot-mockup/src/tests/**`

The mockup should demonstrate:

- raw primitive leaves:

  ```rust
  ValueSlot<bool>
  ValueSlot<[f32; 3]>
  ```

- semantic primitive leaves:

  ```rust
  RatioSlot
  PositiveF32Slot
  SourcePathSlot
  ```

- semantic struct leaves:

  ```rust
  Dim2uSlot
  Affine2dSlot
  ```

- records containing maps/options/enums.

Tests should assert:

- shapes come from `T::value_shape`.
- ids are generated from Rust names.
- authored TOML/JSON still round-trips for the mockup vertical slice.
- duplicate ids are caught where implemented.

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-mockup
cargo test -p lpc-model
```

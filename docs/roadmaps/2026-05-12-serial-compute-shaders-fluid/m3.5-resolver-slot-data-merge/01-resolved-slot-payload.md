# Phase 1: Aggregate Resolved Slot Payload

## Scope

Change resolver productions from leaf-only values to aggregate-capable owned slot data.

Out of scope:

- Merge policy behavior.
- Fluid node.
- UI/probe work.

## Code Organization Reminders

- Prefer one clear concept per file.
- Keep tests at the bottom of files.
- Do not hide new domain concepts in large `mod.rs` files.

## Sub-agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings.
- If blocked, stop and report.

## Implementation Details

- Add `SlotMerge` in `lpc-model/src/slot/slot_merge.rs` and export it.
- Add shape-aware lookup helpers in `lpc-model/src/slot/slot_lookup.rs` so engine can snapshot a sub-slot with the correct shape.
- Evolve `lpc-engine/src/dataflow/resolver/production.rs` so `Production` owns `Rc<SlotData>` plus source.
- Keep leaf convenience APIs:
  - construct from `WithRevision<LpValue>` / `WithRevision<LpsValueF32>`
  - access `value()` / `as_value()` for existing call sites
  - expose `data()` for aggregate users.
- Update engine host production to snapshot runtime state/authored def slots into `SlotData` instead of rejecting non-value slots.
- Preserve existing scalar tests.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine dataflow::resolver
```

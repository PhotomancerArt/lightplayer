# Phase 2: View Slot Mirror

## Scope Of Phase

Add the real generic client-side slot mirror to `lpc-view`.

In scope:

- `lpc-view/src/slot` module.
- Authoritative registry/root/data mirror.
- Apply full sync, registry snapshots, and patches.
- Pending mutation tracking.
- `prepare_set_value` that reads authoritative versions from the mirror.
- `apply_mutation_response`.
- Tests for patch application and pending/rejection behavior.

Out of scope:

- Optimistic local value updates.
- Server mutation application.
- Existing legacy `ProjectView` migration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `no_std + alloc` compatibility.
- Keep the mirror generic; no mockup/domain dependencies.
- Do not add temporary TODOs.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-view/src/lib.rs`
- new `lp-core/lpc-view/src/slot/mod.rs`
- new `lp-core/lpc-view/src/slot/mirror.rs`
- new `lp-core/lpc-view/src/slot/pending.rs`
- new `lp-core/lpc-view/src/slot/apply.rs`
- current mock-only reference:
  `lp-core/lpc-slot-mockup/src/view/mock_client.rs`

`SlotMirrorView` should own:

- `SlotShapeRegistry`
- root name to `SlotShapeId`
- root name to `SlotData`
- pending mutations keyed by `WireSlotMutationId`
- rejected mutation errors keyed by `WireSlotMutationId`

Important behavior:

- Applying a mutation response must not mutate authoritative `roots`.
- `prepare_set_value` should store pending state and return a
  `WireSlotMutationRequest`.
- `expected_shape_version` comes from the root shape's `changed_frame`.
- `expected_data_version` comes from the target value leaf's `changed_frame`.

## Validate

```bash
cargo test -p lpc-view slot
```

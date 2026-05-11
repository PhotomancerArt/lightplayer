# Produced Slot Access

## Scope of phase

Replace representation-split runtime access with one produced-slot access
surface returning `RuntimeProduct`.

In scope:

- Introduce `ProducedSlotAccess` or equivalent.
- Keep `RuntimeProduct` as the produced payload.
- Replace `RuntimePropAccess` / `RuntimeOutputAccess` on `Node` with the new
  produced access surface.
- Update core nodes, resolver host reads, compatibility projection, and tests
  to use the new trait.
- Support `get`, `snapshot`, and `iter_changed_since` on the produced access
  trait, even if some nodes return empty iterators.

Out of scope:

- Final generic client sync model.
- Final slot declaration metadata.
- Binding/query renames except where required by this phase.

## Code organization reminders

- Keep `RuntimeProduct` as the payload concept unless implementation reveals a
  concrete reason to rename.
- Keep temporary compatibility adapters small and documented.
- Put tests at the bottom of files.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-engine/src/prop/runtime_prop_access.rs`
- `lp-core/lpc-engine/src/prop/runtime_output_access.rs`
- `lp-core/lpc-engine/src/prop/mod.rs`
- `lp-core/lpc-engine/src/node/node.rs`
- `lp-core/lpc-engine/src/nodes/core/*.rs`
- `lp-core/lpc-engine/src/project_runtime/*.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`
- `lp-core/lpc-engine/tests/runtime_spine.rs`

Expected changes:

- Add a produced access trait whose entries use `SlotName`, `ValuePath`,
  `RuntimeProduct`, and `FrameId`.
- Update `Node` to expose produced access through one method.
- Migrate existing scalar props by wrapping them as `RuntimeProduct::Value`.
- Migrate existing non-scalar outputs by returning their `RuntimeProduct`
  directly.
- Remove or alias old access traits only if an alias is needed during this
  phase; aliases should not survive final cleanup.

## Validate

```bash
cargo test -p lpc-engine
```

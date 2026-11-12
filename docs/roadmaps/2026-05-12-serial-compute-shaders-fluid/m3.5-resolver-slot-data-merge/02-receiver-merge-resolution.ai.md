# Phase 2: Receiver-Owned Map Merge Resolution

## Scope

Add resolver support for receiver-owned aggregate merge policy and prove `SlotData::Map` merge-by-key through direct and bus bindings.

Out of scope:

- Real fluid node.
- Reverse assembly from child bindings into parent aggregates.
- Persistent client explain/probe UI.

## Code Organization Reminders

- Put merge helpers in a dedicated resolver file if they grow.
- Keep fake/test-only host data in test modules or engine tests.

## Sub-agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings.
- If blocked, stop and report.

## Implementation Details

- Extend `ResolveHost` with:
  - `bindings_for_consumed_slot(node, slot)` defaulting to the old single binding.
  - `merge_policy_for_consumed_slot(node, slot)` defaulting to scalar behavior.
- Add `NodeTree::bindings_for_consumed_slot` returning all bindings at the winning owner depth.
- In `EngineSession::resolve_consumed_slot`, branch on `SlotMerge`:
  - `Latest`: use current single-binding/default path.
  - `Error`: error if multiple bindings exist.
  - `ByKey`: resolve all sources and merge `SlotData::Map` by key.
- For bus sources under merge, collect all providers for the bus rather than selecting a single highest-priority provider.
- Add merge trace events for policy selection and input application.
- Add focused tests with two produced maps merging into one receiver slot, including projection through `emitters[7]` if practical.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine resolver
```

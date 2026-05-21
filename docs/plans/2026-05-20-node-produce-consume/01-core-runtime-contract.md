# Phase 1: Core Runtime Contract

## Scope Of Phase

Introduce `produce(slot)` and `consume()` on `NodeRuntime`, route produced-slot demand through `produce(slot)`, and preserve simple-node ergonomics with a once-per-frame full-evaluation helper.

Out of scope:

- Final radio behavior split.
- Removing resolver cycle workaround code.
- Redesigning radio delivery semantics.

## Code Organization Reminders

- Keep trait-level types near `NodeRuntime`.
- Put helper functions below primary engine dispatch.
- Mark compatibility helpers with a clear TODO if they are temporary.
- Tests stay at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`

Expected changes:

- Add a `ProduceResult` type or equivalent.
- Add `NodeRuntime::produce(slot, ctx)`.
- Add `NodeRuntime::consume(ctx)` with default no-op.
- Replace `tick_node_once_for_output` with produced-slot dispatch that calls `produce(slot)`.
- Add a helper for simple nodes that evaluates full-node behavior once per frame.
- Ensure the helper does not force specialized nodes like radio to resolve unrelated inputs.
- Keep engine status/error reporting equivalent to current `tick()` errors.

Tests to add or update:

- Produced-slot demand calls `produce(slot)`.
- A simple fallback node evaluates once per frame even when multiple produced slots are demanded.
- Existing resolver cache behavior still avoids repeated production.

## Validate

```bash
cargo fmt --package lpc-engine
cargo test -p lpc-engine node_trait_is_object_safe
cargo test -p lpc-engine same_produced_slot_twice_calls_host_once
cargo test -p lpc-engine demand_roots_resolve_inside_resolve_session_while_session_is_live
```

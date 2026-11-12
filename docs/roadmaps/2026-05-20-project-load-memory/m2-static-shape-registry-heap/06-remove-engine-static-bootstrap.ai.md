# Phase 06: Remove Engine Static Bootstrap

## Scope Of Phase

Stop registering static authored shapes into each engine registry. The engine
registry should contain only dynamic/project/runtime shapes.

Out of scope:

- Further node graph memory refactors.
- Direct generated precompiled accessors unless already implemented earlier.

## Code Organization Reminders

- Keep the removal small and easy to review.
- Delete obsolete helper code after all call sites no longer use it.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Main-agent phase preferred because this is the switch-over point.
- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- generated `slot_shapes.rs` API

Expected changes:

- `Engine::with_services` creates an empty dynamic registry.
- `register_authored_slot_shapes` is removed or moved to test/compatibility
  support only.
- Static lookup works through generated catalog fallback.
- Dynamic runtime state shapes still register only when they are truly dynamic.

## Validate

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo test -p fw-tests --test profile_alloc_emu
```

# Phase 04: Engine And Generated Views

## Scope Of Phase

Move engine/runtime call sites and generated slot views onto the lookup
abstraction.

Out of scope:

- Removing static registration from `Engine::with_services`.
- Wire protocol changes.

## Code Organization Reminders

- Keep runtime context APIs narrow and compatible where practical.
- Avoid broad refactors of node runtime behavior.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/render/slot_views.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/slot_mutation.rs`
- `lp-core/lpc-engine/src/nodes/**/*`
- `lp-core/lpc-engine/src/gfx/compute_desc.rs`

Expected changes:

- Generated view compile/get APIs accept `&impl SlotShapeLookup` or a concrete
  compatible lookup type.
- Runtime contexts expose shape lookup without forcing static shape residency.
- Engine code compiles while still using the existing registry bootstrap.

## Validate

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

# Milestone 5: Legacy runtime removal and hardening

## Goal

Remove or quarantine the old `LegacyProjectRuntime` path after M4 has made the
new core runtime the active demo path, then harden validation across host,
emulator, and ESP32 flows.

This is the cleanup and hardening milestone. It should not introduce large new
runtime concepts unless M4/M4.1 prove a small missing piece is required.

## Context

M2 creates the engine. M3 migrates source. M4 ports legacy MVP behavior into
first-class core nodes and switches the server/demo path to the new stack. M4.1
hardens runtime buffer/render-product sync. M5 removes the remaining old runtime
path and validates that the new runtime is durable.

## In scope

- Remove remaining server/CLI/test references to `LegacyProjectRuntime`.
- Retire `LegacyProjectRuntime` and old legacy hooks where possible.
- Remove or quarantine old JSON/runtime compatibility code no longer needed.
- Remove remaining compatibility projection/wiring that M4/M4.1 no longer need.
- Add explicit destroy/unregister/close traversal for runtime teardown as part
of retiring legacy lifecycle assumptions.
- Run broad parity and integration validation.
- Preserve embedded shader compilation and execution.
- Update documentation to reflect the new default runtime architecture.

## Out of scope

- Queryable/sampled render products, unless required to preserve current
behavior.
- New visual node types.
- Async/parallel scheduler execution.
- Large unrelated cleanup outside runtime cutover.

## Key decisions

- **Cleanup after demo cutover:** do not spend M5 designing the active runtime
path; M4 owns the demo switch, and M5 removes what is left behind.
- **One default runtime:** avoid keeping old and new runtimes as long-lived
peers.
- **Validation is the work:** this milestone is expected to spend most of its
risk budget on tests, firmware checks, and integration fallout.

## Suggested plan location

When ready, expand this milestone with `/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m5-core-engine-cutover/`

## Success criteria

- `LegacyProjectRuntime` is retired or quarantined and unused by main runtime
paths.
- The core engine remains the default path for server/CLI/test runtime behavior.
- Existing legacy MVP scenarios still render correctly.
- Required host, emulator, and ESP32 validation commands pass.
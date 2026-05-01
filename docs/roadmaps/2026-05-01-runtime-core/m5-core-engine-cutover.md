# Milestone 5: Core engine cutover

## Goal

Make the new core engine the default runtime, retire the old
`LegacyProjectRuntime` path, and validate that host, emulator, and ESP32 flows
still work.

This is the integration milestone. It should not introduce large new runtime
concepts unless M4 proves a small missing piece is required for cutover.

## Context

M2 creates the engine. M3 migrates source. M4 ports legacy MVP behavior onto the
core engine. M5 removes the parallel runtime path and makes the new engine the
runtime used by apps/tests.

## In scope

- Switch server/CLI/test wiring to the core engine.
- Retire `LegacyProjectRuntime` and old legacy hooks where possible.
- Remove or quarantine old JSON/runtime compatibility code no longer needed.
- Update sync/view wiring as needed for the new engine's frame/version model.
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

- **Cutover after parity:** do not retire the old engine until M4 proves the MVP
  path on the new engine.
- **One default runtime:** avoid keeping old and new runtimes as long-lived
  peers.
- **Validation is the work:** this milestone is expected to spend most of its
  risk budget on tests, firmware checks, and integration fallout.

## Suggested plan location

When ready, expand this milestone with `/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m5-core-engine-cutover/`

## Success criteria

- `LegacyProjectRuntime` is retired or no longer used by the main runtime path.
- The core engine is the default path for server/CLI/test runtime behavior.
- Existing legacy MVP scenarios still render correctly.
- Required host, emulator, and ESP32 validation commands pass.


# Phase 05: Wire Static Catalog Export

## Scope Of Phase

Make shape read/sync export static catalog shapes on demand without registering
them into the engine registry. Normal responses can keep the existing
`SlotShapeRegistrySnapshot` payload shape and include static descriptors plus
dynamic overlay entries.

Out of scope:

- Client UI polish beyond ordinary missing-shape errors.
- Removing engine static registration.

## Code Organization Reminders

- Keep wire structs version-tolerant with defaults where possible.
- Keep direct JSON writing allocation-conscious.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/slot/slot_shape_registry_json.rs`
- `lp-core/lpc-engine/src/engine/project_read_shapes.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-cli/src/debug_ui/*`

Expected behavior:

- Full sync and shape reads include static catalog descriptors plus dynamic
  registry entries.
- Static descriptors are converted/exported only while building or streaming a
  client response.
- Internal dynamic-only snapshots remain available for callers that truly want
  only runtime-owned shapes.
- No catalog fingerprint/mismatch protocol is required for this milestone.

## Validate

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo test -p lpc-wire
```

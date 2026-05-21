# Phase 4: Status And Readback

## Scope Of Phase

Ensure node-local reload failures are visible through existing project-read and
tree delta paths so the UI can show errors without relying on server logs.

Out of scope:

- Building the UI error presentation itself.
- Changing wire protocol unless existing fields are insufficient.

## Code Organization Reminders

- Prefer using existing `WireNodeStatus`.
- Keep serialization behavior in project-read/tree sync modules.
- Add tests close to existing project-read tests.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Inspect and update as needed:

- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-engine/src/node/sync.rs`
- `lp-core/lpc-wire/src/tree/wire_tree_delta.rs`

Expected behavior:

- A status change to `WireNodeStatus::Error(message)` appears in tree deltas.
- Detail-level node reads still include available node def slots for bad nodes
  when the authored TOML parsed successfully.
- Runtime state slots are absent for fresh-load failed nodes with no runtime.
- Hot-reload failed nodes with old runtime may still expose runtime state, but
  status must show error.

Add tests:

- Load invalid SVG fixture project; project-read nodes detail includes fixture
  status error.
- Hot reload valid to invalid; project-read since previous revision returns a
  tree delta with fixture status error.
- Streaming project-read matches non-streaming response for the error case.

## Validate

```bash
cargo test -p lpc-engine project_read
cargo test -p lpc-wire
```

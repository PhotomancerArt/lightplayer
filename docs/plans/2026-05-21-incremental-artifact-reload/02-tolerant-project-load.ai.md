# Phase 2: Tolerant Project Load

## Scope Of Phase

Make initial project load tolerate node-local prepare failures. Invalid fixture
SVG mapping should not make `ProjectLoader::load_from_root` fail. Instead, the
fixture node should exist and carry `WireNodeStatus::Error`.

Out of scope:

- Incremental reload from file changes.
- Shader source dependency tracking.
- Project-level failures such as unreadable `project.toml`.

## Code Organization Reminders

- Keep project structure parsing in `project_loader.rs`.
- Extract small helper functions for per-node prepare/attach if needed.
- Do not hide new behavior in a large `mod.rs`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Refactor fixture runtime setup in
`lp-core/lpc-engine/src/engine/project_loader.rs`:

- Replace fatal `let mapping = resolve_fixture_mapping(...)?;` with a
  node-local prepare helper.
- On success, attach `FixtureNode` and set status `Ok`.
- On failure, do not return `ProjectLoadError`.
- Mark the fixture tree entry with `WireNodeStatus::Error(message)`.
- Put the entry state into `NodeEntryState::Failed { reason }` or leave it
  pending if that better matches existing behavior. Prefer `Failed` if project
  read already serializes it correctly.
- Continue registering/processing other nodes where possible.

Generalize the helper enough to use for other node attach failures:

```rust
fn mark_node_load_error(runtime: &mut Engine, node_id: NodeId, frame: Revision, message: String)
```

Fresh invalid SVG scenario:

- `ProjectLoader::load_from_root` returns `Ok(Engine)`.
- The fixture node appears in `tree_deltas`.
- Fixture status is `WireNodeStatus::Error`.
- Shader/output/other independent nodes still load.

Add tests in `lp-core/lpc-engine/src/engine/project_loader.rs`:

- Project with fixture SVG source whose SVG has no path/polyline.
- Assert load returns `Ok`.
- Assert fixture node status is `Error` and message mentions SVG mapping.
- Assert unrelated nodes are alive or present as expected.

## Validate

```bash
cargo test -p lpc-engine invalid_svg
cargo test -p lpc-engine project_loader
```

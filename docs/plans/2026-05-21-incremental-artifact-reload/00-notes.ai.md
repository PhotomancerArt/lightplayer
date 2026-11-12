# Incremental Artifact Reload Notes

## Scope

Fix project hot-reload so bad authored node artifacts or dependent source files
do not crash the server and do not tear down the entire runtime. A failed SVG
fixture mapping should become a fixture node error state while the last good
runtime, and other nodes, continue running.

This plan covers:

- Transactional reload safety for the current server path.
- Tolerant project loading where node-local prepare failures become
  `WireNodeStatus::Error`.
- Incremental artifact/source reload so ordinary file saves do not rebuild the
  whole engine.
- Tests for invalid SVG mapping during initial load and hot edit.

This plan does not cover:

- A full UI redesign for displaying errors.
- General-purpose persistent diagnostics beyond existing `WireNodeStatus`.
- Precompiling shaders or weakening the on-device JIT path.

## User Notes

- The whole engine should not reload on ordinary file change.
- The engine should start even if one node is bad.
- A failed SVG mapping should not cause runtime failure.
- Invalid intermediate SVG files are expected while testing/editing.
- The earlier panic was:
  `project runtime is only absent while reloading` from
  `lp-app/lpa-server/src/project.rs`.

## Current Broken Path

- `lp-app/lpa-server/src/server.rs` collects filesystem changes under each
  loaded project, then calls `project.reload()` for any non-empty change set.
- `lp-app/lpa-server/src/project.rs::Project::reload` drops
  `self.runtime` before loading the replacement engine:
  `drop(self.runtime.take())`.
- If `ProjectLoader::load_from_root` fails, `runtime` remains `None`.
- Later tick/project-read paths call `project.engine_mut()` or
  `project.engine()`, both of which `expect("project runtime is only absent
  while reloading")`, causing a panic.

## Current Loader Behavior

- `lp-core/lpc-engine/src/engine/project_loader.rs` loads the whole authored
  project into one new `Engine`.
- Fixture SVG mapping is resolved during project load:
  `resolve_fixture_mapping(root, &node.source_base_path, &config)?`.
- If SVG parsing/mapping fails, it returns `ProjectLoadError::InvalidSourcePath`
  and aborts the whole load.
- This makes fixture mapping a project-fatal operation even though it is a
  node-local prepare concern.

## Existing Pieces To Reuse

- `ArtifactStore` already has `LoadError`, `PrepareError`, and
  `ResolutionError` states.
- `NodeEntry` already carries a wire-visible `WireNodeStatus`.
- Runtime tick/produce errors already restore the node runtime and set
  `WireNodeStatus::Error` instead of killing the engine.
- `tree_deltas_since` already includes status changes, so UI/project-read can
  see node status updates.

## Open Questions

## What should happen on invalid `project.toml`?

- **Context:** `project.toml` defines the tree structure. If it is unreadable,
  there may not be a coherent new tree to build.
- **Suggested answer:** Treat `project.toml` as project-level. For hot reload,
  keep the previous engine alive and expose a project/server reload error. Do
  not claim a specific node failed unless the old tree can map the edited path
  to a node.

## What should happen on invalid node TOML?

- **Context:** A node artifact path maps to a node id in the existing engine.
- **Suggested answer:** Keep the old node runtime alive if one exists, update
  the artifact state to `LoadError`, and set that node status to
  `WireNodeStatus::Error`. On fresh load, create the node entry with failed
  state/status and let the rest of the project run.

## What should happen on invalid dependent source files such as SVG or GLSL?

- **Context:** These are prepare/compile inputs for a node, not project
  structure.
- **Suggested answer:** Reprepare only dependent nodes. On failure, keep the old
  runtime for hot reload, set node status `Error`, and record artifact/source
  error context. On fresh load, the node exists but starts failed/unavailable;
  other nodes continue.

## Should failed nodes produce stale outputs?

- **Context:** For hot reload, retaining the last good runtime preserves live
  output. For fresh load there is no previous runtime.
- **Suggested answer:** Hot reload keeps the last good runtime alive but marks
  the node status as error. Fresh load has no runtime for the bad node, so
  consumers may report missing/failed input, but the engine itself stays alive.

## Relevant Files

- `lp-app/lpa-server/src/server.rs`
- `lp-app/lpa-server/src/project.rs`
- `lp-app/lpa-server/tests/fs_version_tracking.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/artifact/artifact_store.rs`
- `lp-core/lpc-engine/src/artifact/artifact_state.rs`
- `lp-core/lpc-engine/src/node/node_entry.rs`
- `lp-core/lpc-wire/src/project/wire_node_status.rs`

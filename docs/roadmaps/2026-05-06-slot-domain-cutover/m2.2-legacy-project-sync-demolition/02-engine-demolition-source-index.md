# Phase 2: Engine Demolition And Source Index

## Scope Of Phase

In scope:

- Delete engine legacy detail projection.
- Remove `CoreProjectRuntime::get_changes` legacy response path.
- Rename/reframe `CompatibilityProjection` as a source authoring index.
- Preserve source loaded config/path snapshots that canonical source sync will
  need in M3.
- Preserve resource projection helpers that are not inherently legacy.

Out of scope:

- Implementing canonical project sync.
- Runtime state/params/output slot roots.
- Rewriting project loader behavior beyond names and retained index shape.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Rename files/types when a concept is retained but its old name is wrong.
- Keep helpers lower in files and tests at the bottom.
- Mark any disabled sync entry point with a clear TODO for M3.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-engine/src/project_runtime/detail_projection.rs`
- `lp-core/lpc-engine/src/project_runtime/compatibility_projection.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/resource_projection.rs`
- `lp-core/lpc-engine/src/project_runtime/mod.rs`
- `lp-core/lpc-engine/tests/scene_update.rs`
- `lp-core/lpc-engine/tests/partial_state_updates.rs`
- `lp-core/lpc-engine/tests/scene_render.rs`
- `lp-core/lpc-engine/tests/get_changes_resource_projection.rs`

Expected changes:

- Delete `detail_projection.rs` and remove it from `project_runtime/mod.rs`.
- Rename `compatibility_projection.rs` to something like
  `source_authoring_index.rs`.
- Rename `CompatibilityProjection` to a non-legacy name such as
  `SourceAuthoringIndex`.
- Remove `clone_as_node_config_box` if it only served `LegacyNodeDetail`.
- Keep APIs to query source path and loaded source def by `NodeId`; these should
  be ready for M3 source root snapshots.
- Remove `CoreProjectRuntime::get_changes` if it only returns
  `LegacyProjectResponse`, or replace it with an explicit disabled stub that
  does not use legacy wire types.
- Keep project loading, node insertion, ticking, and resource stores working.
- Keep resource projection helpers unless they require legacy detail state.
- Delete legacy-detail-only engine tests. Preserve or split resource tests that
  exercise resource summaries/payloads without legacy details.

Edge cases:

- `project_loader.rs` may rely on `LoadedNodeConfig::clone_as_node_config_box`
  only for legacy detail. If source index needs typed access later, keep the
  enum and add accessor methods instead of boxing trait objects.
- Some tests may combine scene rendering with legacy detail assertions. Keep
  rendering tests only if they can validate runtime behavior without legacy
  response objects.

## Validate

Run:

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine --lib
git diff --check
```

If integration tests are deleted or deferred, record which canonical coverage
must return in M3-M6.


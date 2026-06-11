# Engine Cutover Story Test Missing

- **Severity:** P2
- **Status:** open
- **First seen:** 2026-06-10-api-design-review.md
- **Last reviewed:** 2026-06-10-api-design-review.md
- **Owner:** unassigned

## Finding

The registry has good slice tests, but no single test demonstrates the full behavior needed before engine cutover: load a root artifact, discover both inline and file-backed child nodes, apply a wire-shaped overlay with every supported mutation family, commit, reload, and prove the committed/effective graph is what the engine would consume.

## Evidence

- `lp-core/lpc-node-registry/tests/project_diff.rs:30` - project diff tests prove snapshot-to-overlay-to-commit equivalence, but they operate through `diff()`/`ArtifactOverlay`, not a wire-shaped edit sequence.
- `lp-core/lpc-node-registry/tests/pending_sync.rs:21` - pending sync tests cover small `SyncOp` batches, including one slot edit plus commit, but not a complete project load/edit/reload workflow.
- `lp-core/lpc-node-registry/tests/asset_overlay.rs:32` - asset overlay tests cover materializing pending asset create/replace/delete before commit.
- `lp-core/lpc-node-registry/tests/slot_overlay.rs:41` - slot overlay tests cover value edits and inline child projection.
- `lp-core/lpc-node-registry/tests/commit_promotion.rs:39` - commit tests cover slot/asset flush and inline child change details in separate scenarios.
- `lp-core/lpc-engine/src/engine/project_loader.rs:755` - the engine still loads project TOML directly through its existing loader path.
- `lp-core/lpc-engine/src/engine/slot_mutation.rs:19` - the engine still owns the current direct slot mutation path.

## Impact

Without a story test, it is hard to tell whether the registry API is actually ready for engine consumption or merely internally plausible. This is exactly the sort of broad workflow that can fail at the boundaries: node address mapping, inline child paths, child file references, asset freshness, pending overlay projection, commit results, and post-commit reload can all be individually green while still not composing into the engine cutover path.

## Suggested Fix

Add a focused `lpc-node-registry` integration test, for example `tests/engine_cutover_story.rs`, that uses a memory filesystem and a wire-shaped adapter once issue 03 exists.

The scenario should include:

- Root `project.toml` or `playlist.toml` loaded from an artifact.
- At least one inline child def and one `ref` child def in a separate file.
- At least one shader/fixture asset reference registered through `referenced_asset_paths`.
- Overlay setup through the future wire-shaped API, not direct `ArtifactOverlay` construction.
- Slot value edit via `AssignValue`.
- Structural creation via `EnsurePresent` that adds a node/map entry.
- Structural deletion via `Remove` that deletes a node/map entry.
- Kind/variant change via `EnsurePresent` on a variant path.
- Asset create, replace, and delete.
- Commit.
- Fresh registry reload from filesystem proving the committed artifacts are sufficient for engine loading.
- Assertions on `SyncOutcome`, `NodeDefUpdates`, `DefChangeDetail`, effective reads, materialized source, and final filesystem bytes.

## Validation

- `cargo test -p lpc-node-registry --test engine_cutover_story`
- `cargo test -p lpc-node-registry`

## History

- 2026-06-10: opened by Codex API design review.

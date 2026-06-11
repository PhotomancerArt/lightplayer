# Review - codex/incremental-artifact-reload API Design

## Target

- **Branch/PR:** codex/incremental-artifact-reload
- **Base:** origin/codex/incremental-artifact-reload (`a9c644b341554cf14e3f0767262d56e60baa8dcc`)
- **Head:** `13f928ef76ac52dc2e4ce8a92baa62cb92f34c90`
- **Review date:** 2026-06-10
- **Reviewer:** Codex

## Summary

`lpc-node-registry` now has a solid local prototype for artifact inventory, parsed node definitions, pending overlay edits, effective reads, commit promotion, and diff-driven project changes. The current tests prove these pieces in useful slices. They do **not** yet prove the whole engine cutover workflow, mostly because the new registry edit model has no real wire contract and the current `lpc-wire` mutation API is still the old value-leaf-only engine mutation path.

The short answer to "can we show the full functionality in a test today?" is **not yet**. We can show most underlying mechanics with direct registry APIs, but not the requested "using on-wire API" story.

## Findings

| Issue | Severity | Status | Summary |
| --- | --- | --- | --- |
| [03-wire-edit-contract-missing.md](03-wire-edit-contract-missing.md) | P1 | open | New registry edits have no durable wire request/response contract; current wire mutation only supports value leaves. |
| [04-engine-cutover-story-test-missing.md](04-engine-cutover-story-test-missing.md) | P2 | open | Existing tests cover slices, but no single test proves the full root-load, child-discovery, edit, asset CRUD, commit, reload path needed for engine cutover. |
| [05-sync-errors-are-lossy.md](05-sync-errors-are-lossy.md) | P1 | open | Filesystem sync and committed refresh paths can swallow registry errors and report no-op/default results. |

## What Works Now

| Capability | Current state | Evidence |
| --- | --- | --- |
| Root artifact load | Implemented through `NodeDefRegistry::load_root`; root path must be absolute, root artifact is registered, reachable children/assets are registered. | `lp-core/lpc-node-registry/src/registry/load.rs:14`, `lp-core/lpc-node-registry/src/registry/load.rs:30`, `lp-core/lpc-node-registry/src/registry/load.rs:33` |
| Inline child discovery | Implemented via model-level `NodeDef::invocation_sites` and registry recursive registration. | `lp-core/lpc-model/src/nodes/node_def.rs:217`, `lp-core/lpc-node-registry/src/registry/load.rs:65`, `lp-core/lpc-node-registry/src/registry/load.rs:90` |
| File-backed child discovery | Implemented for `NodeInvocation::Ref`, resolving relative artifact specifiers and registering child artifact roots. | `lp-core/lpc-node-registry/src/registry/load.rs:68`, `lp-core/lpc-node-registry/src/registry/load.rs:72`, `lp-core/lpc-node-registry/src/registry/load.rs:78` |
| Asset discovery | Implemented for shader, compute shader, and fixture refs through `NodeDef::referenced_asset_paths`. | `lp-core/lpc-model/src/nodes/node_def.rs:246`, `lp-core/lpc-node-registry/src/registry/inventory.rs:280` |
| Pending overlay | Implemented as `ArtifactOverlay`, keyed by `ArtifactLoc`, with current pending slot edits or one pending asset edit per artifact. | `lp-core/lpc-node-registry/src/edit_model/artifact_overlay.rs:12`, `lp-core/lpc-node-registry/src/edit_model/artifact_overlay.rs:18`, `lp-core/lpc-node-registry/src/edit_model/artifact_overlay.rs:74` |
| Slot edit language | Simplified to `EnsurePresent`, `AssignValue`, and `Remove`. | `lp-core/lpc-node-registry/src/edit_model/slot_edit.rs:8` |
| Auto-create/defaulting | `AssignValue` first calls `apply_ensure_present`; `EnsurePresent` delegates to model slot mutation helpers. | `lp-core/lpc-node-registry/src/edit_apply/slot_edit_apply.rs:53`, `lp-core/lpc-node-registry/src/edit_apply/slot_edit_apply.rs:56`, `lp-core/lpc-node-registry/src/edit_apply/slot_edit_apply.rs:97` |
| Effective reads | Implemented through `NodeDefView`, `effective_state`, `read_effective_bytes`, and `materialize_source`. | `lp-core/lpc-node-registry/src/registry/effective_read.rs:19`, `lp-core/lpc-node-registry/src/registry/effective_read.rs:61`, `lp-core/lpc-node-registry/src/registry/effective_read.rs:80`, `lp-core/lpc-node-registry/src/registry/effective_read.rs:85` |
| Commit | Implemented: writes pending bytes/deletes to fs, refreshes affected defs, reconciles artifacts, clears overlay. | `lp-core/lpc-node-registry/src/registry/commit.rs:17`, `lp-core/lpc-node-registry/src/registry/commit.rs:38`, `lp-core/lpc-node-registry/src/registry/commit.rs:44`, `lp-core/lpc-node-registry/src/registry/commit.rs:82` |
| Diff to overlay | Implemented under the `diff` feature for snapshot equivalence tests. | `lp-core/lpc-node-registry/src/diff/project_diff.rs:17` |

## Test Coverage Map

| Needed behavior | Covered today? | Notes |
| --- | --- | --- |
| Load root node from artifact | Yes | `project_diff.rs:a1_roundtrip_load_root_after_commit`; registry unit tests also cover root load. |
| Load inline child node | Yes | `slot_overlay.rs:c2_inline_child_slot_patch_visible_in_view`; `commit_promotion.rs:c2_inline_child_changed_after_commit`. |
| Load referenced child node from separate file | Partial/yes | `fs_change_semantics.rs:s5b_path_child_parse_error_reports_entered_error` creates a ref child and syncs errors. No combined story test. |
| Register referenced shader/fixture assets | Yes | `fs_change_semantics.rs:s2_glsl_edit_only_bumps_artifact_store_revision` and `s3_svg_edit_only_bumps_artifact_store_revision`. |
| Set up overlay | Yes, local API | `overlay_lifecycle.rs` and `pending_sync.rs`; not wire API. |
| Value leaf edit | Yes | `slot_overlay.rs:c1_setslot_patches_clock_rate_in_view`, `commit_promotion.rs:d2_commit_updates_committed_and_clears_overlay`. |
| Structural create / add node | Partial | `EnsurePresent` exists and diff can create artifacts, but no explicit node-add story test using a wire-like API. |
| Structural delete / delete node | Partial | `Remove` exists and overlay conflict cleanup is tested, but no explicit node-delete story test using a wire-like API. |
| Kind/variant change | Partial | Filesystem kind change is tested; slot-driven kind/variant change is not covered in a full workflow. |
| Asset create | Yes pre-commit | `asset_overlay.rs:c4a_add_asset_via_overlay_implicit_create`; commit path is less explicit outside diff tests. |
| Asset replace | Yes | `asset_overlay.rs:c4c_replace_glsl_via_overlay_def_unchanged`, `commit_promotion.rs:d2_commit_setbytes_updates_committed`. |
| Asset delete | Yes pre-commit | `asset_overlay.rs:c4b_delete_asset_via_overlay`; commit-delete story should be explicit. |
| Commit changes | Yes | `commit_promotion.rs` and `pending_sync.rs`. |
| Reload committed state as engine would | Partial | `project_diff.rs:a1_roundtrip_load_root_after_commit` reloads after diff commit, but not after all mutation families. |
| On-wire API | No | Current `lpc-wire` mutation is still `WireSlotMutationRequest` + `SetValue`; registry `SyncOp` is local and not serde/schema-ready. |

## Current Terms

- **Artifact**: a file-like authored thing tracked by the registry, usually `.toml`, `.glsl`, `.svg`, or similar.
- **Asset**: non-def artifact body edited as bytes/text, such as shader source or fixture mapping. Registry code now generally uses `AssetEdit` for this.
- **Source**: still valid when it means authored shader/source text (`SourceFileSlot`, `materialize_source`). Avoid using it for generic artifact bookkeeping.
- **ArtifactLoc**: registry identity for an artifact, currently path-backed for these workflows.
- **ArtifactStore**: registry-owned catalog of artifact locations, revisions, and transient read state.
- **NodeDefLocation**: identity of a parsed node definition: artifact location plus `SlotPath` inside that artifact. Root defs use the root path; inline defs use child invocation paths.
- **NodeDefEntry**: current parsed registry entry: `NodeDefLocation`, `NodeDefState`, and revision.
- **NodeDefState**: either a loaded `NodeDef` or a parse/error placeholder.
- **NodeDefRegistry**: owner of committed parsed defs plus pending overlay; the main local API for load, sync, effective read, and commit.
- **ArtifactOverlay**: pending current-state map keyed by `ArtifactLoc`. It is not an append-only edit log.
- **ArtifactEdits**: one artifact's pending state: ordered slot edits or one asset edit.
- **SlotEdit**: structural/value slot edit within a `.toml` artifact: `EnsurePresent`, `AssignValue`, or `Remove`.
- **AssetEdit**: asset body operation: `None`, `ReplaceBody(Vec<u8>)`, or `Delete`.
- **SyncOp**: local registry ingress enum mixing filesystem events, pending edits, remove/clear, and commit. This is not a settled wire type.
- **SyncOutcome**: result of processing `SyncOp`s: committed changes plus a pending-changed bit.
- **SyncResult**: factual committed registry changes: `NodeDefUpdates` plus `NodeDefChangeDetail`s.
- **NodeDefUpdates**: added/changed/removed def locations.
- **NodeDefChangeDetail**: coarse change classification: content, kind changed, entered error, left error.
- **NodeDefView**: read-only effective projection over registry committed state plus overlay.
- **ProjectSnapshot / diff**: host/test harness tools that compute an `ArtifactOverlay` between filesystem snapshots.

## Half-Baked Or Still Moving

- **Wire vocabulary**: old `WireSlotMutation` still exists; new registry edits are not exposed as `lpc-wire` request/response types.
- **Revision/concurrency**: registry pending edits do not yet carry client ids, base revisions, conflict policy, or idempotency semantics.
- **`SyncOp` naming/boundary**: useful local test API, but too broad for wire because it includes `FsEvent`.
- **Error policy**: parse errors are modeled, but some sync/inventory/reconcile errors are swallowed.
- **Engine identity mapping**: the registry uses artifact path plus `SlotPath`; the engine/UI still often think in `node.<id>.def` roots.
- **Full generic topology**: `NodeDef::invocation_sites` is centralized in `lpc-model`, but it is still explicit for `Project` and `Playlist`, not a fully generic slot-shape walker.
- **Compatibility facade**: `edit` re-exports `edit_model`/`edit_apply` for stability; this is fine temporarily but should not become the conceptual home.
- **Commit helper naming**: internal names like `commit_slot_overlay` still carry older slot-overlay vocabulary.
- **Source/asset wording**: authored shader source APIs should stay named source; registry bookkeeping should continue moving toward artifact/asset/definition location.

## Suggested Next Step

Do not cut the engine over yet. First, define the wire edit contract and add the engine-cutover story test against a registry adapter. Once that test is green locally, the engine cutover has a concrete target instead of a moving API.

## Validation

- `cargo test -p lpc-node-registry`: passed with existing dead-code warnings in shared integration-test helpers.

## Notes

- The worktree had pre-existing local changes before this review: `artifact_location.rs` is renamed to `artifact_loc.rs`, and `artifact/mod.rs` points at the new module name. This review did not modify those code changes.
- Existing issue files `01` and `02` remain fixed and were not reopened.

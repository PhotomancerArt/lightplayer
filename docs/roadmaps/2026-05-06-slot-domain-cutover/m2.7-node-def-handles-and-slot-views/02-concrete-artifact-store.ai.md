# Phase 2 - Concrete ArtifactStore

## Scope Of Phase

Make `ArtifactStore` own loaded `NodeDef` payloads directly.

In scope:

- Remove public `ArtifactStore<A>`, `ArtifactEntry<A>`, and `ArtifactState<A>`
  generics from engine-domain artifact storage.
- Make `ArtifactState::{Loaded, Prepared, Idle}` carry `lpc_model::NodeDef`.
- Update tests that used `ArtifactStore<i32>` to use small `NodeDef` fixtures or
  move generic-cache tests out if they no longer describe the domain.
- Update `Engine` from `ArtifactStore<()>` to `ArtifactStore`.
- Update artifact docs to describe the store as authored node-definition
  lifecycle, not arbitrary payload caching.

Out of scope:

- Loading every artifact through the store. That is Phase 3.
- Consumed-slot fallback. That is Phase 4.
- Renaming files from `artifact_manager.rs` to `artifact_store.rs` unless it is
  small and keeps the filesystem map clearer.

## Code Organization Reminders

- File names should match primary types where practical. If renaming
  `artifact_manager.rs` to `artifact_store.rs`, update `mod.rs` in the same
  phase.
- Keep `ArtifactStore`, `ArtifactEntry`, and `ArtifactState` in separate files.
- Keep tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If test fixtures become too noisy, stop and report rather than inventing fake
  domain concepts.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/artifact/artifact_manager.rs`
- `lp-core/lpc-engine/src/artifact/artifact_entry.rs`
- `lp-core/lpc-engine/src/artifact/artifact_state.rs`
- `lp-core/lpc-engine/src/artifact/source_loader.rs`
- `lp-core/lpc-engine/src/artifact/mod.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

Expected changes:

- `ArtifactStore::load_with` returns/stores `NodeDef`.
- `ArtifactStore::entry` returns `ArtifactEntry` with concrete state.
- `content_frame` naming may remain as-is for this phase, but do not introduce
  more frame/version confusion.
- Existing source-loader helpers may be removed, narrowed, or left for Phase 3
  if they still compile cleanly.

## Validate

```bash
cargo test -p lpc-engine artifact
cargo test -p lpc-model
```


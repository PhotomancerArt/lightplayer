# Phase 3 - Loader Artifact Payloads And Def Handles

## Scope Of Phase

Load `NodeDef` payloads into `ArtifactStore` and attach runtime nodes with
`NodeDefHandle`.

In scope:

- Add `NodeDefHandle`.
- Store `NodeDefHandle` on `NodeEntry`.
- Parse project and child artifacts into `NodeDef`.
- Load project and child `NodeDef` payloads into `ArtifactStore`.
- Remove or shrink engine-local `LoadedNodeDef`.
- Reduce or delete `SourceAuthoringIndex` if `ArtifactStore` now provides the
  canonical authored def payload.

Out of scope:

- Consumed-slot fallback behavior. That is Phase 4.
- TextureNode config migration. That is Phase 5.
- Inline node defs; non-root handle paths should remain unsupported.

## Code Organization Reminders

- Put `NodeDefHandle` in its own file:
  `lp-core/lpc-engine/src/node/node_def_handle.rs`.
- Keep loader helper functions lower in `project_loader.rs` or split them if the
  file gets harder to scan.
- Avoid large `mod.rs` implementations.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- Preserve user changes in nearby files.
- Report any temporary compatibility wrappers left behind.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/node/node_def_handle.rs`
- `lp-core/lpc-engine/src/node/mod.rs`
- `lp-core/lpc-engine/src/node/node_entry.rs`
- `lp-core/lpc-engine/src/node/node_tree.rs`
- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/source_authoring_index.rs`
- `lp-core/lpc-engine/src/project_runtime/mod.rs`
- `lp-core/lpc-engine/src/lib.rs`

Suggested `NodeDefHandle`:

```rust
pub struct NodeDefHandle {
    artifact: ArtifactId,
    path: SlotPath,
}
```

Add constructors/accessors:

- `NodeDefHandle::artifact_root(artifact: ArtifactId)`
- `artifact()`
- `path()`
- `is_artifact_root()`

Loader direction:

- Load `project.toml` as `NodeDef::Project`.
- Load child node files as `NodeDef::{Texture, Shader, Output, Fixture}`.
- Keep matching on variants where the loader needs to attach concrete runtime
  node types.
- Entry creation should use the handle, not a bare artifact id, as the source
  definition identity.

## Validate

```bash
cargo test -p lpc-engine project_loader
cargo test -p lpc-engine runtime_spine
```


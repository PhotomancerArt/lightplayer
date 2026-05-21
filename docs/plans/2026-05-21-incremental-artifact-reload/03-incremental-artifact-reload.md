# Phase 3: Incremental Artifact Reload

## Scope Of Phase

Replace ordinary full-engine reload on file change with engine-owned
incremental artifact/source reload.

Out of scope:

- Sophisticated graph diffing for arbitrary project structure edits.
- UI rendering changes.
- New persistence format.

## Code Organization Reminders

- Prefer a new file such as
  `lp-core/lpc-engine/src/engine/artifact_reload.rs`.
- Keep file-change collection in server code, but move reload decisions into
  engine code.
- Keep dependency tracking explicit and search-friendly.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add an engine API:

```rust
pub struct ProjectFsChange {
    pub path: LpPathBuf,
    pub change_type: FsChangeType,
}

pub struct ProjectReloadReport {
    pub node_errors: Vec<(NodeId, String)>,
    pub project_errors: Vec<String>,
}
```

Exact type names can follow existing conventions.

Update `lp-app/lpa-server/src/server.rs`:

- Pass project-relative changes to `project.apply_fs_changes(...)`.
- Stop calling `project.reload()` for every non-empty change set.
- Keep transactional full reload only for project-structural changes that are
  not yet incrementally supported.

Add source dependency tracking in `Engine`:

- Map project-relative source path to affected `NodeId`s.
- Record shader source path dependencies.
- Record fixture SVG mapping source dependencies.
- Update the index after successful node artifact reload or prepare.

Handle changed paths:

- Node TOML path: reload that node artifact and reprepare the node.
- Shader source path: recompile/reprepare dependent shader node.
- SVG source path: re-resolve mapping and reprepare dependent fixture node.
- Unknown path: ignore or report a non-fatal project warning.
- `project.toml`: use transactional full reload fallback for now.

Node runtime replacement must be transactional:

- Prepare the replacement runtime first.
- On success, swap into the node entry and set status `Ok`.
- On failure, leave old runtime alive, set status `Error(message)`, and store
  artifact/source error metadata.

Add tests:

- Hot edit valid SVG to invalid: tick continues, fixture status becomes error,
  old runtime remains alive.
- Hot edit invalid SVG back to valid: fixture status returns to `Ok`, runtime
  replacement occurs.
- Hot edit shader GLSL to invalid: shader node status error, old compiled
  runtime remains alive; valid edit clears error.

## Validate

```bash
cargo test -p lpc-engine artifact_reload
cargo test -p lpa-server --test fs_version_tracking
cargo check -p lpc-engine
cargo check -p lpa-server
```

# Incremental Artifact Reload Design

## Scope

Make file changes and authored artifact failures node-local wherever possible.
The server must not panic or drop the whole runtime because a fixture SVG,
shader source, or node artifact is temporarily invalid.

## File Structure

```text
lp-app/lpa-server/src/
  project.rs              # transactional full reload fallback, runtime access safety
  server.rs               # route filesystem changes into engine incremental reload

lp-app/lpa-server/tests/
  fs_version_tracking.rs  # reload failure keeps runtime alive, version advances

lp-core/lpc-engine/src/engine/
  project_loader.rs       # tolerant initial load; node-local prepare errors
  artifact_reload.rs      # new incremental artifact/source reload entry points
  engine.rs               # node runtime replacement/status helpers
  project_read_nodes.rs   # verify status/slot visibility for failed nodes

lp-core/lpc-engine/src/artifact/
  artifact_store.rs       # preserve/use error states during reload
  artifact_state.rs       # existing state vocabulary

lp-core/lpc-engine/src/nodes/fixture/
  fixture_node.rs         # fixture runtime remains normal; bad SVG is prepare failure
  mapping/svg_path/       # mapping parser errors remain typed and non-panicking
```

## Architecture Summary

The server should stop treating ordinary project file changes as "destroy and
recreate the entire engine." Instead, file changes flow into the existing
engine as artifact/source invalidations.

There are three layers of behavior:

1. **Transactional safety:** Until the incremental path is complete, any full
   reload fallback builds a replacement engine before touching the current
   runtime. Failure leaves the old runtime alive.
2. **Tolerant load:** Project loading builds the tree and as many runtimes as
   possible. Node-local failures set node status to `WireNodeStatus::Error`
   and do not abort the project load.
3. **Incremental reload:** Changed files invalidate only affected artifacts or
   dependent source files. A successful prepare swaps in the new node runtime.
   A failed prepare records error state/status while keeping the old runtime.

## Main Components

## Server File Change Handling

`LpServer::tick` should pass `project_changes` to the project/engine instead of
calling unconditional `Project::reload()`. The server still owns filesystem
version advancement. Once changes are attempted, advance `last_fs_version` past
the processed version even if some node reloads fail, so an invalid intermediate
file does not loop forever.

## Project Runtime Safety

`Project::reload` becomes transactional as an immediate fallback. It should
never leave `runtime = None` after returning. `engine()` and `engine_mut()` can
remain infallible once that invariant is restored, but tests should prove reload
failure does not make them panic.

## Engine Incremental Reload

Add an engine API along these lines:

```rust
pub struct ProjectFsChange {
    pub path: LpPathBuf,
    pub change_type: FsChangeType,
}

impl Engine {
    pub fn apply_project_fs_changes<R>(
        &mut self,
        root: &R,
        changes: &[ProjectFsChange],
        frame: Revision,
    ) -> ProjectReloadReport
    where
        R: ArtifactReadRoot + ?Sized;
}
```

The implementation classifies paths:

- `project.toml`: project-structural. Use transactional full reload fallback
  for now, or report a project-level reload error.
- Node artifact TOML: reload that node definition, then prepare/swap that node.
- Dependent source artifacts: reprepare nodes that declared dependency on that
  path, for example shader source files and SVG fixture mappings.

## Dependency Index

During initial load and successful prepare, record source dependencies:

- Shader node depends on external `ShaderSource::Path`.
- Fixture node depends on `MappingConfig::SvgPath.source`.
- Future nodes can add dependencies through the same mechanism.

This index is path-based and project-relative. It is used only to choose
affected nodes on file changes; it is not a replacement for bindings.

## Node Prepare And Swap

Add helper APIs that prepare a node runtime without destroying the old one:

- `try_prepare_node_runtime(...) -> Result<Box<dyn NodeRuntime>, NodeErrorLike>`
- `replace_node_runtime(node_id, runtime, revision)`
- `mark_node_error(node_id, message, revision)`

On hot reload:

- Success: swap runtime, update artifact state, set status `Ok`.
- Failure: keep existing runtime if present, set status `Error(message)`, store
  artifact/source error state.

On fresh project load:

- Success: attach runtime, status `Ok`.
- Failure: create tree entry, set status `Error(message)`, state failed/pending,
  continue loading other nodes.

## Fixture SVG Mapping

Move SVG mapping resolution out of fatal project-load flow and into fixture
node preparation. The mapping parser should continue returning normal errors;
the caller converts them to fixture node status.

Fresh load with invalid SVG:

- Project loads.
- Fixture node appears in tree.
- Fixture node status is `Error("resolve svg fixture mapping: ...")`.
- Other nodes load and tick.

Hot edit to invalid SVG:

- Old fixture runtime remains alive.
- Fixture node status becomes `Error(...)`.
- When SVG becomes valid again, fixture runtime is replaced and status returns
  to `Ok`.

## Validation Strategy

Add tests at three levels:

- Server reload failure keeps runtime alive and advances processed FS version.
- Engine load with invalid SVG still returns `Ok(Engine)` and marks fixture
  node error.
- Hot editing a valid SVG to invalid keeps old fixture runtime alive, then
  editing back to valid clears the error.

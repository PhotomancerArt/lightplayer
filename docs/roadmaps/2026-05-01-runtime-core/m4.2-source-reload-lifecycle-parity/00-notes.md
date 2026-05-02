# M4.2 notes: source reload and lifecycle parity

## Current shortcut

`CoreProjectRuntime::handle_fs_changes` accepts changes but does not rebuild,
recompile, or remove anything. Tests currently document this as M4 behavior. The
server can advance filesystem versions, but the runtime does not yet reflect
edits.

Unload and stop-all also drop core runtimes without an explicit destroy walk.
That is acceptable for M4 scaffolding but not for full parity.

## Likely implementation shape

- Keep a source index from legacy `/src/*.kind` directories to core `NodeId`s.
- Classify filesystem changes into config changes, shader source changes,
  creation, deletion, and unknown changes.
- Rebuild only the affected runtime node when possible.
- Invalidate dependent nodes or cached products when a producer changes.
- Add a core runtime teardown path that calls `Node::destroy` and lets
  `RuntimeServices` unregister/close output sinks.

## Questions to answer during planning

- Should reload preserve `NodeId` for an existing authored path, or remove and
  recreate entries when the kind changes?
- How should render products and runtime buffers be invalidated when a node is
  rebuilt?
- Is folder discovery still shallow for M4.2, or should deeper discovery land
  here because creation/deletion needs it?
- Where should output sink ownership live so teardown does not depend on
  `OutputNode` being a demand participant?

## Validation focus

- `node.toml` edit updates config-visible behavior.
- GLSL edit recompiles and changes rendered output.
- Node deletion removes the node from core lookup and sync projection.
- Unload/stop-all closes output handles exactly once.

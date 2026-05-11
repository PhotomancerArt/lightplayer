# M4.2 notes: source reload and lifecycle parity

## Scope of work

M4.2 makes the core runtime respond to authored source changes and lifecycle
events with enough parity that M5 can remove the legacy runtime without hiding
reload or teardown regressions.

In scope:

- Rebuild affected core nodes when a loaded node's `node.toml` changes.
- Recompile shader nodes when their GLSL source changes.
- Remove core nodes when authored node files or directories are deleted.
- Create core nodes for newly authored node directories when dependencies are
satisfiable.
- Add an explicit core teardown path that calls `Node::destroy`.
- Close and unregister output sinks during node deletion, project unload, and
stop-all.
- Preserve `lpa-server` filesystem version tracking semantics.
- Replace the current M4 no-op scene update tests with real reload, deletion,
and teardown assertions.

Out of scope:

- Runtime buffer and render-product sync shape changes owned by M4.1.
- Multi-shader/shared texture render-order behavior owned by M4.3.
- Removing `LegacyProjectRuntime`, owned by M5.
- Broad redesign of the node graph, resolver, or source layout.

## Current state of the codebase

`CoreProjectRuntime::handle_fs_changes` currently accepts `FsChange` values and
returns `Ok(())` without rebuilding, recompiling, deleting, or creating nodes.
The server can still advance per-project filesystem versions because
`lpa-server` only updates `Project::last_fs_version` when the runtime hook
returns success.

`CoreProjectLoader::load_from_root` already discovers legacy `/src/*.kind`
directories, creates corresponding core tree entries, stores a legacy source
directory to `NodeId` index in `CoreProjectRuntime::legacy_src_dirs`, attaches
runtime nodes in dependency order, records compatibility snapshots, and wires
fixture demand bindings. Most rebuild work should reuse or extract this logic
rather than inventing a parallel loader.

Long-term, reload should be understood as node-scoped source reconciliation,
not data-flow invalidation. A node entry has authored source/config identity,
runtime payload, children, and eventually resource slot state. Changing a node's
authored file should update that node entry's source/config/artifact version and
only recreate the runtime payload when the payload cannot apply the change in
place. Downstream data-flow users should see changed props/resources through
normal frame/versioned reads, not because they were rebuilt.

The core tree already has `NodeTree::remove_subtree`, but it tombstones entries
without calling `Node::destroy` and without letting `RuntimeServices` close or
unregister output sink handles. The engine has `attach_runtime_node`, but no
destroy-aware replace/remove API.

`RuntimeServices` owns output sink state and lazy-opened
`OutputChannelHandle`s. `OutputNode` only allocates the backing sink buffer and
does not own the provider handle. Teardown should therefore live in
`RuntimeServices`, with node destroy handling node-owned resources only.

M4.1 added shared resource identity and sync projection. Its decisions matter
for M4.2: ids are not reused for the lifetime of a loaded runtime, and removal
invalidates resources while recreation gets new ids. M4.2 should avoid changing
the M4.1 wire/resource envelope unless a reload bug proves it necessary.

The node-runtime roadmap notes also describe children as lifecycle-owned node
entries (`Input`, `Sidecar`, `Inline`) and mention cache invalidation on config
version bumps. M4.2 does not need to build child reload, but its reload design
should leave room for config changes to create/destroy/reload children later.

Existing `scene_update` tests document the no-op behavior:

- `node.toml` changes are accepted but do not emit config updates.
- GLSL changes keep the existing shader active.
- node deletion is ignored by core runtime reload.

These tests should flip to the M4.2 behavior as phases land.

## Questions that need to be answered

### Q1: Preserve `NodeId` on same-kind reload?

Context: `legacy_src_dirs` maps authored `/src/...` paths to core `NodeId`s`,
and clients already use handles for detail and resource requests.

Suggested answer: preserve `NodeId` when an existing authored path reloads with
the same node kind. Treat deletion, path recreation, and kind change as remove
plus create with a new `NodeId`.

Status: unresolved.

### Q2: Preserve resource ids on rebuild?

Context: M4.1 decided ids are not reused after removal, but same-node rebuilds
can either preserve existing resource ids or allocate replacements. Preserving
ids is friendlier for clients but may require replace APIs on resource stores.

Answer: do not require resource id preservation for M4.2. It is a nice-to-have,
but the right long-term shape is probably to store node-owned resource slot
state on the `NodeTree` / `NodeEntry` side and let replacement runtime payloads
bind to existing slots. For M4.2, correctness is that reload produces valid
resources and sync projection reflects the current ids; removal or kind change
invalidates old resources and allocates new ids.

Status: resolved.

### Q3: What does reload mean for data-flow dependents?

Context: Data-flow references are resolved at runtime through props/resources.
Reloading a texture should not rebuild shaders merely because shaders read its
props; a changed prop/resource version should be enough. What will matter later
is child lifecycle reload, because config/artifact changes may create, remove,
or retarget children.

Answer: reload is node-scoped for M4.2. Update/reload the changed node entry and
its runtime payload as needed, but do not rebuild data-flow dependents. Preserve
room for future child reload by treating child creation/destruction as lifecycle
work owned by the changed node, not by graph invalidation.

Status: resolved.

### Q4: Should creation/deletion support nested source directories?

Context: `CoreProjectLoader::tree_path_for_legacy_src_dir` and
`discover_legacy_node_dirs` already support mapping nested source paths into
folder spines, but M4.2 can keep discovery behavior narrow if creation/deletion
tests only require the existing authored layout.

Suggested answer: keep whatever `discover_legacy_node_dirs` already supports;
do not add deeper discovery semantics unless a failing creation/deletion test
shows the current source discovery is insufficient.

Status: unresolved.

### Q5: Where should output close ownership live?

Context: `RuntimeServices` owns registered output sinks and their
`OutputChannelHandle`s. `OutputNode` allocates a runtime buffer but does not own
the open provider channel.

Suggested answer: add close/unregister APIs to `RuntimeServices`, and have
core runtime teardown/deletion call those APIs. Keep `OutputNode::destroy`
simple unless it gains actual provider-owned state later.

Status: unresolved.

### Q6: Should filesystem version advance on reload errors?

Context: `lpa-server` updates `last_fs_version` only when
`handle_fs_changes` returns `Ok(())`. If a GLSL recompile fails, repeated ticks
could retry the same bad source forever unless the runtime records the error
and still accepts the change.

Suggested answer: match legacy shader reload behavior: record compile/load
errors in node status where possible and return `Ok(())` for handled authored
changes, reserving `Err` for unexpected internal failures that should not
advance the version.

Status: unresolved.
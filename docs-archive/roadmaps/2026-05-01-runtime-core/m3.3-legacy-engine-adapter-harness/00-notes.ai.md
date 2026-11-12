# Scope of Work

M3.3 is cancelled as an implementation milestone and superseded by M4.

The original idea was to add a temporary legacy adapter harness before porting
the full shader -> fixture -> output path. That now looks like the wrong
direction: the old MVP nodes are useful product nodes, and M4 should rework them
as durable core `Node` implementations instead of wrapping legacy runtime APIs
with an adapter layer that would be removed shortly afterward.

In scope for this superseded note:

- Record the decision not to build the temporary adapter harness.
- Point M4 at a clean-break node port.
- Preserve useful codebase findings for M4 planning.

Out of scope:

- Any M3.3 implementation.
- Thin adapters or dummy parity scaffolding.
- Old-vs-new runtime harnesses built around preserving `LegacyProjectRuntime`.
- Buffer sync; that remains M4.1.

# Current State

## Legacy Project Loading

Legacy project files currently load from:

- `/project.json` for `lpc_model::ProjectConfig`;
- `/src/<id>.<kind>/node.toml` for each legacy node config;
- shader artifacts such as `main.glsl`.

Relevant source-loading pieces:

- `lp-core/lpc-source/src/legacy/node_loader.rs` defines
  `LegacyNodeReadRoot`, `discover_legacy_node_dirs`, and
  `load_legacy_node_config`.
- `lp-core/lpc-source/src/legacy/node_config_file.rs` defines legacy node path
  rules.
- `lp-base/lpfs/src/lpc_source_legacy.rs` implements `LegacyNodeReadRoot` for
  filesystem views.
- `lp-core/lpc-engine/src/legacy_project/legacy_loader.rs` wraps the source
  loader for engine-side legacy loading.

## Legacy Runtime Path

`LegacyProjectRuntime` is still the authoritative old runtime path:

- `lp-core/lpc-engine/src/legacy_project/project_runtime/core.rs`
  owns the runtime container and calls into legacy project helpers.
- `lp-core/lpc-engine/src/legacy/project.rs` initializes and ticks legacy node
  runtimes.
- Runtime init order is texture -> shader -> fixture -> output.
- Ticking is fixture-demand driven: fixtures render, shaders are lazily rendered
  through texture requests, fixture sampling mutates output buffers, then dirty
  outputs flush through the output provider.

Useful observables for parity:

- `MemoryOutputProvider` byte snapshots.
- `ProjectResponse::GetChanges` / `NodeState` snapshots.
- `ProjectView::frame_id` and node detail/state updates.
- Existing integration tests such as `scene_render.rs`, `scene_update.rs`, and
  `partial_state_updates.rs`.

## Core Engine Path

The core `Engine` currently owns:

- `NodeTree<Box<dyn Node>>`;
- `BindingRegistry`;
- `Resolver` and frame cache;
- `RenderProductStore`;
- `RuntimeBufferStore`;
- demand roots.

The core engine has no filesystem/project bootstrap yet. Current tests build
graphs in memory through `EngineTestBuilder` and dummy nodes in
`lp-core/lpc-engine/src/engine/test_support.rs`.

M4 should bridge that gap while porting real node behavior, rather than adding a
temporary adapter layer first.

## Runtime Buffers

M3.2 added `RuntimeBufferStore` and `RuntimeProduct::Buffer(RuntimeBufferId)`.
For M3.3/M4, the intended buffer pattern is:

- allocate a stable runtime buffer ID for a logical node product;
- mutate the store entry in place per frame where possible;
- bump the `Versioned<RuntimeBuffer>` frame on changes;
- avoid per-frame IDs and avoid duplicate authoritative frame buffers;
- project/copy only at compatibility boundaries.

M4 should use stable buffer handles directly in the ported core nodes. M4.1 will
handle proper sync refs, metadata/version exposure, and client cache behavior.

# Decision

- **Decision:** Skip the M3.3 adapter harness.
- **Why:** Temporary adapters would preserve legacy runtime shapes that M4 wants
  to replace. The useful work is porting the old MVP nodes into the new core
  system and validating runtime buffers/products directly.
- **M4 implication:** M4 should be allowed to break the branch while it makes a
  clean source-to-engine cut and ports texture/shader/fixture/output as
  first-class core nodes.

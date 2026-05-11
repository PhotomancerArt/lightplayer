# M4.3 — Runtime spine notes

# Scope of work

Land the runtime spine contracts that sit on top of the M4.2 tree/schema
types and the M4.3a/M4.3b crate cleanup:

- `lpc-engine::Node` trait and runtime contexts.
- `TickContext` access to resolver cache, bus, artifact/version state, and
  read-only tree metadata.
- Runtime node entry fields for `SrcNodeConfig`, artifact id/state, and
  resolver cache.
- `ArtifactManager` / `ArtifactLocation` / `ArtifactId` state machine in
  `lpc-engine`.
- Binding resolver for consumed slots (`params` / `inputs`), producing
  `LpsValueF32` into `ResolverCache`.
- Slot-view helpers for reading resolved values across the consumed
  namespaces.
- Generalized authored artifact loading hooks that use `lpc-source`
  instead of the current legacy-only `node.json` loader.

This plan is the bridge between the M4.2 data structures and later M4.4
domain/sync work. It should avoid cutting the legacy visual runtime over
wholesale; legacy node port/cutover remains later work.

M4.4 is intentionally under-specified relative to the spine. If a small
piece of the M4.4 placeholder is needed to make the M4.3 spine coherent,
it is acceptable to pull it forward rather than preserve an artificial
milestone boundary.

Working milestone split:

- **M4.3:** engine-side runtime spine. Owns runtime contracts, node
  lifecycle, artifact manager, binding resolution, resolver cache,
  `TickContext`, and produced-value access through `RuntimePropAccess`.
- **M4.4:** sync + view projection. Owns wire deltas for produced props,
  client/view prop caches, `lpc-view` application of those deltas, and
  editor-facing mirror behavior.

If a runtime-side hook is required for sync later, M4.3 may add it. If a
wire/view mirror is not required to prove engine behavior, leave it for
M4.4.

# Current state

## Roadmap/design state

- `m4.3-runtime-spine/plan.md` summarizes landed phases 01–07 and pointers
  to design docs.
- `design/02-node.md` defines the intended `Node` trait:
  `tick`, `destroy`, `handle_memory_pressure`, and `props`.
- `design/03-artifact.md` defines the aspirational artifact manager states
  and refcounting; the M4.3 implementation uses `ArtifactLocation` as the
  resolved cache key and `ArtifactId` as the dense runtime handle.
- `design/04-config.md` defines `NodeConfig` conceptually; after
  M4.3a/M4.3b this maps to `lpc_source::SrcNodeConfig` and
  `SrcArtifactSpec`.
- `design/05-slots-and-props.md` and
  `design/06-bindings-and-resolution.md` define consumed vs produced
  namespaces, `RuntimePropAccess`, resolver cache, bus, and binding
  resolution.

Some design prose still has historical names such as `PropAccess`,
`LpsValue`, or `ArtifactSpec`. The implementation should use current
post-M4.3b names:

- `RuntimePropAccess`
- `LpsValueF32`
- `ModelValue` / `ModelType`
- `SrcArtifact` / `SrcArtifactSpec` / `SrcNodeConfig` / `SrcBinding`
- `ArtifactLocation` / `ArtifactId`
- `WireNodeSpecifier` / `WireSlotIndex`
- `ProjectView` / `PropAccessView`

## Existing `lpc-engine` runtime paths

There are two partially overlapping runtime structures:

1. **Legacy runtime path**
   - `lpc-engine/src/project/project_runtime/types.rs` defines the
     current `ProjectRuntime` with:
     - `nodes: BTreeMap<NodeId, NodeEntry>`
     - legacy `NodeEntry { path, kind, config: Box<dyn lpl_model::NodeConfig>, runtime: Option<Box<dyn NodeRuntime>>, ... }`
   - `lpc-engine/src/nodes/node_runtime.rs` defines the old
     `LegacyNodeRuntime` trait with `init`, `render`, `destroy`,
     `shed_optional_buffers`, downcasting, `update_config`, and
     `handle_fs_change`.
   - `lpc-engine/src/runtime/contexts.rs` defines old
     `NodeInitContext` and `RenderContext` traits tied to legacy
     texture/output resolution and frame time.
   - `lpc-engine/src/project/legacy_loader.rs` is the legacy filesystem
     loader: it discovers `/src/*.texture|*.shader|*.output|*.fixture`
     and parses each `node.json` into a `Box<dyn lpl_model::NodeConfig>`.

2. **New spine data path**
   - `lpc-engine/src/tree/` defines generic `NodeTree<N>`,
     `NodeEntry<N>`, `EntryState<N>`, `TreeError`, and tree-delta
     generation. `NodeEntry` currently has commented future fields for
     config, artifact, and resolver cache.
   - `lpc-engine/src/resolver/` defines `ResolverCache`,
     `ResolvedSlot`, `ResolveSource`, and `BindingKind` as data shapes.
     It does not yet resolve bindings.
   - `lpc-engine/src/bus/` defines `Bus` and `ChannelEntry` with
     claim/publish/read APIs over `LpsValueF32`.
   - `lpc-engine/src/prop/runtime_prop_access.rs` defines
     `RuntimePropAccess` over produced `LpsValueF32` values.
   - `lpc-engine/src/wire_bridge/` converts
     `LpsValueF32 -> ModelValue` and `ModelType -> LpsType`.

M4.3 should connect the new spine data path without trying to delete the
legacy runtime path.

## Existing `lpc-source` source model

- `SrcNodeConfig` holds:
  - `artifact: SrcArtifactSpec`
  - `overrides: Vec<(PropPath, SrcBinding)>`
- `SrcArtifact` is a source-side trait with `KIND`, `CURRENT_VERSION`,
  `schema_version`, and `walk_slots`.
- `load_artifact` exists in `lpc-source::artifact` and handles typed
  TOML/serde loading plus schema-version validation.
- Source value specs materialize portable `ModelValue` shapes and can be
  converted at the engine boundary.

## M4.3 implementation status (engine spine)

The following were scope for phases 01–07 and are implemented in
`lpc-engine` (see `plan.md` and phase files):

- `node`: `Node`, `TickContext`, `DestroyCtx`, `MemPressureCtx`,
  `PressureLevel`, `NodeError`.
- `tree::NodeEntry`: `SrcNodeConfig`, `ArtifactId`, `ResolverCache`.
- `artifact`: manager/location/id/state machine; `load_source_artifact`
  delegating file-backed locations to `lpc_source::load_artifact`.
- `resolver`: cascade (`resolve_slot`), `ResolverContext`, `ResolveError`.

Still intentionally **not** done in M4.3 (later milestones):

- `NodeTree<Box<dyn Node>>` as the primary runtime owner inside
  `ProjectRuntime` (M5).
- Wire/view produced-prop mirroring and `PropsChanged` (M4.4).
- `#[derive(RuntimePropAccess)]` / `lpc-derive`.
- Legacy project loader remains `project::legacy_loader` (`node.json`).

# Questions

## Confirmation-style questions

| # | Question | Context | Suggested answer |
| --- | --- | --- | --- |
| Q1 | Treat M4.3 as additive spine infrastructure, not legacy runtime cutover? | `ProjectRuntime` and `NodeRuntime` are still serving current tests/app paths. | Yes: build the new modules and adapters, leave full cutover to M5/M4.4. |
| Q2 | Put new `Node`/context/error types in `lpc-engine/src/node/` or `lpc-engine/src/spine/` rather than reusing `nodes/`? | `nodes/` currently contains legacy runtime node implementations and `NodeRuntime`. | Use `node/` (singular) for the new spine contracts; keep `nodes/` legacy. |
| Q3 | Keep `Node::props() -> &dyn RuntimePropAccess` and not create a `PropAccess` alias? | M4.3b aligned names and removed old aliases. | Yes: use `RuntimePropAccess` everywhere. |
| Q4 | Defer `lpc-derive` / `#[derive(RuntimePropAccess)]` to a later plan? | M4.3 needs the trait usable; deriving can be separate and proc-macro crates add churn. | Yes: hand-written test impls are enough for M4.3. |
| Q5 | Make `Node` not require `Send + Sync` for now? | ESP32/no_std and current single-engine runtime do not need thread movement; old `NodeRuntime` has `Send + Sync` but that may overconstrain embedded nodes. | Use no `Send + Sync` bound initially unless a concrete caller needs it. |
| Q6 | Store `SrcNodeConfig` on new `NodeEntry` rather than inventing an engine-side config wrapper? | `SrcNodeConfig` is already the authored per-instance shape and M4.3 is not cutting over domains yet. | Yes, store `SrcNodeConfig` directly. |
| Q7 | Use an owned/string error enum for new spine errors instead of tying to legacy `crate::error::Error`? | Legacy `Error` is project/runtime specific; new spine should be reusable. | Yes: add `NodeError` / `ArtifactError` / `ResolveError` as focused `alloc::string::String`-carrying errors. |

## Discussion-style questions

### Q-A — How much of `ArtifactManager` should M4.3 implement?

Options:

1. Full generic `ArtifactManager<A>` with refcounting and states
   (`Resolved`, `Loaded`, `Prepared`, `Idle`, error states), but no
   domain trait.
2. Minimal manager data model plus tests for refcount/state transitions;
   actual loading delegated to a caller-supplied closure.
3. Skip manager implementation and only add entry fields/placeholders.

Suggested answer: option 2. Implement the state/refcount mechanics and a
closure-based `load` path using `SrcArtifactSpec`, but do not introduce
`ProjectDomain` yet. That gives M4.4/M5 a real manager to build on
without forcing the domain abstraction early.

### Q-B — What should binding resolution support in M4.3?

Options:

1. Full resolver over `SrcBinding`, `SrcSlot` trees, bus reads, node
   output reads, artifact defaults, cache invalidation, warnings.
2. Focused resolver for literal/default/bus with `NodeProp` target
   validation but limited target-node resolution.
3. Only data structures and context methods; no resolution behavior.

Suggested answer: option 2. It exercises the cascade and cache shape while
not requiring the full domain/tree wake semantics. `NodeProp` can validate
`outputs`-only and return a structured unresolved result until M4.4/M5
wires produced-prop sync.

### Q-C — Should `TickContext` borrow the full `NodeTree<Box<dyn Node>>`
or a smaller view?

Options:

1. Borrow the full mutable tree and let context methods manage access.
2. Borrow a small `TreeReadView` / resolver-facing access object to avoid
   broad mutable access during `tick`.
3. Skip tree access until M4.4.

Suggested answer: option 2. M4.3 can define a narrow context shape with
bus, current frame, resolver cache, artifact info, and read-only tree
lookup. Avoid letting `tick` restructure the tree.

### Q-D — Should new `NodeTree` become the owner inside current `ProjectRuntime` now?

Options:

1. Replace legacy `ProjectRuntime.nodes: BTreeMap<NodeId, legacy NodeEntry>`
   with the new `NodeTree<Box<dyn Node>>` immediately.
2. Add the new spine tree/runtime types alongside current
   `ProjectRuntime`; keep legacy runtime unchanged.
3. Wrap legacy entries in the new `NodeTree` but keep old APIs.

Suggested answer: option 2. The existing tests and app paths depend on the
legacy map and `NodeRuntime`. M4.3 should make the new spine compile and be
unit-testable, then M5/M4.4 can cut over deliberately.

### Q-E — How should source artifact loading relate to legacy `project/loader.rs`?

Options:

1. Replace `project/loader.rs` with generalized `lpc-source` loading.
2. Add new `artifact_loader` / `source_loader` helpers in `lpc-engine`
   and leave the legacy loader alone.
3. Move all loading to `lpc-source`.

Suggested answer: option 2. `lpc-source` owns typed artifact parsing,
while `lpc-engine` owns runtime manager/loading orchestration. The legacy
loader continues until the legacy node port/cutover.

# Notes

- The M4.3 plan should be a full plan in this directory, not a standalone
  `docs/plans/` plan.
- Roadmap-backed plan files stay in place after completion; do not archive
  to `docs/plans-old/`.
- Confirmation answers accepted:
  - Q1: M4.3 is additive spine infrastructure, not legacy runtime cutover.
  - Q2: New spine contracts go in `lpc-engine/src/node/`; existing
    `lpc-engine/src/nodes/` remains legacy.
  - Q3: Use `RuntimePropAccess` directly; do not add a `PropAccess`
    compatibility alias.
  - Q4: Defer `lpc-derive` / `#[derive(RuntimePropAccess)]`.
  - Q5: Do not require `Send + Sync` on the new `Node` trait unless a
    concrete caller needs it.
  - Q6: Store `SrcNodeConfig` directly on new spine entries.
  - Q7: Use focused new spine errors instead of tying new contracts to
    legacy `crate::error::Error`.
- User renamed the legacy runtime trait from `NodeRuntime` to
  `LegacyNodeRuntime` for clarity. M4.3 should preserve that split:
  `node/` for new contracts, `nodes/` for legacy runtime implementations.
- Q-A accepted: implement `ArtifactManager<A>` state/refcount mechanics and
  a closure-based load path in M4.3, but do not introduce `ProjectDomain`
  yet. The manager should be real enough to test acquire/release refs,
  `Resolved -> Loaded`, `Loaded/Prepared -> Idle`, error states, and
  `content_frame` bumps.
- User note: it is OK to bring in more from M4.4 if that produces a more
  solid runtime spine. M4.4 currently feels under-specified, so M4.3 should
  not avoid necessary sync/domain-facing pieces just because they were
  tentatively listed there.
- Milestone boundary refined: M4.3 should focus on the engine runtime
  spine; M4.4 should focus on sync and `lpc-view`. Runtime produced-prop
  access (`RuntimePropAccess`) and `NodeProp` dereference belong in M4.3;
  wire `PropsChanged` and client prop-cache mirroring can stay in M4.4.
- Q-B accepted/refined: M4.3 resolver should support runtime-side
  `NodeProp` dereference from an alive target's `RuntimePropAccess`, plus
  literal, bus, and default resolution. M4.4 owns carrying produced props
  over the wire and mirroring them in `lpc-view`.
- Q-C accepted: `TickContext` should expose a narrow capability/view shape
  rather than borrowing the full mutable `NodeTree`. Node hooks can resolve
  consumed values and observe read-only runtime/tree/artifact state, but
  cannot restructure topology.
- Q-D accepted: M4.3 should keep the new `NodeTree`/`Node` spine alongside
  the current legacy `ProjectRuntime` storage. `ProjectRuntime` cutover and
  legacy node port belong in M5 (`m5-node-spine-cutover.md`).
- Legacy naming note: it is acceptable to rename additional old-path types
  with a `Legacy*` prefix during M4.3 if it makes the side-by-side boundary
  clearer. Do this only where it improves clarity and does not turn M4.3
  into a broad legacy refactor.
- User renamed the old loader path to `project/legacy_loader.rs` and old
  loader functions toward `legacy_*`. M4.3 should add new source/artifact
  loader orchestration alongside that path rather than replacing it.
- Q-E accepted by direction: generalized source artifact loading belongs
  in new `lpc-engine/src/artifact/` orchestration and should leave the
  legacy `project/legacy_loader.rs` path serving current runtime code
  until M5 cutover.

# M4.3 — Runtime Spine Design

# Scope of work

M4.3 lands the engine-side runtime spine in `lpc-engine`, building on:

- M4.2 tree/schema primitives (`NodeTree`, `NodeEntry`,
  `ResolverCache`, `Bus`, `RuntimePropAccess`).
- M4.3a crate split (`lpc-model`, `lpc-source`, `lpc-wire`,
  `lpc-engine`, `lpc-view`).
- M4.3b name alignment (`ModelValue`, `Src*`, selective `Wire*`,
  `*View`).

The milestone creates additive, unit-testable spine infrastructure. It does
not cut the current legacy `ProjectRuntime` over to the new tree; that is
M5 (`m5-node-spine-cutover.md`).

In scope:

- New `lpc-engine::node` contracts:
  - `Node`
  - `TickContext`
  - `DestroyCtx`
  - `MemPressureCtx`
  - `PressureLevel`
  - `NodeError`
- Runtime artifact cache:
  - `ArtifactManager<A>`
  - `ArtifactRef`
  - `ArtifactEntry<A>`
  - `ArtifactState<A>`
  - `ArtifactError`
- New spine entry fields for source config, artifact ref, and resolver
  cache.
- Engine-side binding resolution for consumed slots:
  - per-instance `SrcNodeConfig.overrides`
  - artifact bind/default layer
  - literal/bus/default resolution
  - `NodeProp` dereference through target `RuntimePropAccess`
- Narrow runtime context/view objects so `tick` can resolve values without
  mutating topology.
- Source artifact loading orchestration in `lpc-engine` that works beside
  the existing legacy loader.

Out of scope:

- Replacing current legacy `ProjectRuntime.nodes` storage.
- Porting `Texture` / `Shader` / `Output` / `Fixture` legacy runtimes to
  `Node`.
- Retiring `LegacyNodeRuntime`.
- Wire/view produced-prop mirroring (`PropsChanged`, `lpc-view` prop
  cache), except for engine-side hooks needed to support it later.
- `ProjectDomain` generic runtime cutover unless a small preparatory type
  is needed by the engine spine.
- `lpc-derive` / `#[derive(RuntimePropAccess)]`.

# File structure

```text
lp-core/lpc-engine/src/
├── node/                                # NEW: engine spine contracts
│   ├── mod.rs
│   ├── node.rs                          # Node trait
│   ├── contexts.rs                      # TickContext, DestroyCtx, MemPressureCtx
│   ├── node_error.rs                    # NodeError
│   └── pressure_level.rs                # PressureLevel
│
├── artifact/                            # NEW: runtime artifact state/cache
│   ├── mod.rs
│   ├── artifact_manager.rs              # ArtifactManager<A>
│   ├── artifact_ref.rs                  # ArtifactRef
│   ├── artifact_entry.rs                # ArtifactEntry<A>
│   ├── artifact_state.rs                # ArtifactState<A>
│   └── artifact_error.rs                # ArtifactError
│
├── resolver/
│   ├── mod.rs
│   ├── resolver.rs                      # NEW: binding cascade implementation
│   ├── resolver_context.rs              # NEW: narrow resolver access facade
│   ├── resolver_cache.rs                # UPDATE: helpers for runtime resolver
│   ├── resolved_slot.rs                 # UPDATE: dependency/source frame if needed
│   ├── resolve_source.rs                # UPDATE: NodeProp provenance if needed
│   └── binding_kind.rs
│
├── tree/
│   ├── node_entry.rs                    # UPDATE: source config/artifact/cache fields
│   ├── node_tree.rs
│   ├── entry_state.rs
│   └── ...
│
├── project/
│   ├── legacy_loader.rs                 # EXISTING: legacy path, leave in place
│   └── ...
│
└── nodes/                               # EXISTING: legacy runtimes
    └── node_runtime.rs                  # LegacyNodeRuntime
```

# Conceptual architecture summary

```text
SrcNodeConfig + SrcArtifactSpec
        │
        ▼
ArtifactManager<A> ── acquire/load/release ── ArtifactRef
        │                                      │ content_frame
        ▼                                      ▼
NodeTree<Box<dyn Node>> ─ NodeEntry ─ resolver_cache
        │                    │
        │                    ├─ config: SrcNodeConfig
        │                    ├─ artifact: ArtifactRef
        │                    └─ state: Pending | Alive(Box<dyn Node>) | Failed
        │
        ▼
TickContext
  ├─ resolve(params/inputs) via Resolver
  ├─ bus read/write
  ├─ artifact_changed_since
  ├─ read-only tree/resolver facade
  └─ frame_id

Resolver
  override SrcBinding
    > artifact bind
      > slot default
  Literal/Bus/NodeProp/Default -> LpsValueF32 -> ResolverCache

Node::props() -> RuntimePropAccess
  produced outputs/state for engine-side NodeProp dereference
  wire/view mirroring waits for M4.4
```

# Main components

## `node`

The new `node` module owns engine-spine contracts. It is intentionally
separate from `nodes`, which remains the legacy runtime implementation
module during M4.3.

Target trait shape:

```rust
pub trait Node {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError>;
    fn destroy(&mut self, ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError>;
    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError>;
    fn props(&self) -> &dyn RuntimePropAccess;
}
```

The trait does not expose identity or tree mutation. The contexts carry
only the capabilities needed for each hook. No `Send + Sync` bound is added
until a concrete caller needs it.

## `artifact`

`ArtifactManager<A>` owns runtime artifact state and refcounts without
introducing `ProjectDomain` yet. It should be generic over the loaded
artifact payload type and accept closure-based loading/preparation hooks.

Required behavior:

- Acquire a `SrcArtifactSpec` and create/reuse an entry.
- Track refcount.
- Transition `Resolved -> Loaded`.
- Transition `Loaded` / `Prepared` to `Idle` when refcount reaches zero.
- Preserve error states.
- Bump `content_frame` on successful reload/load.

`ArtifactRef` is the runtime handle stored on new spine `NodeEntry` values.
It exposes enough metadata for `TickContext::artifact_changed_since`.

## `tree`

M4.3 extends the existing generic `NodeTree<N>` / `NodeEntry<N>` data path
so new spine entries can carry:

- `SrcNodeConfig`
- `ArtifactRef`
- `ResolverCache`
- existing lifecycle/status/frame counters

The current legacy `ProjectRuntime` storage remains unchanged. M5 performs
the storage cutover.

## `resolver`

The resolver computes consumed slot values and stores them in
`ResolverCache`.

Resolution priority:

1. `SrcNodeConfig.overrides[prop]`
2. artifact slot `bind`
3. artifact slot default

M4.3 supports:

- `SrcBinding::Literal` -> materialize/convert to `LpsValueF32`
- `SrcBinding::Bus` -> read from `Bus`
- `SrcBinding::NodeProp` -> validate target namespace is `outputs`, then
  dereference target produced props through `RuntimePropAccess`
- default materialization through source slot/value-spec helpers

`NodeProp` dereference is engine-side only in M4.3. M4.4 owns sending
produced prop changes over `lpc-wire` and mirroring them in `lpc-view`.

## `TickContext`

`TickContext` exposes a narrow capability surface:

- current node id
- current frame id
- `resolve`
- `changed_since`
- `artifact_changed_since`
- bus read/write helpers
- read-only tree/target-prop lookup through resolver-facing facades

It must not expose full mutable `NodeTree` access. Node hooks cannot add or
remove children.

## Legacy path

The old path is explicitly legacy-named:

- `nodes::LegacyNodeRuntime`
- `project::legacy_loader`
- current `ProjectRuntime` map-backed storage

M4.3 may add small `Legacy*` renames where that makes the side-by-side
boundary clearer, but it should not turn into a broad legacy refactor. M5
is the cutover milestone.

# Milestone boundary

M4.3 owns engine runtime behavior. M4.4 owns sync/view projection.

M4.3 may add runtime-side hooks that M4.4 will consume, such as produced
prop iteration through `RuntimePropAccess`. M4.4 should own the wire/view
mirror of those values: produced-prop deltas, `PropAccessView` cache updates,
and editor-facing client state.

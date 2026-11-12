# M2.7 Notes - Node Def Handles And Slot Views

## Scope

This milestone should close the next runtime modeling gap: runtime nodes must read authored configuration through the resolver, not by holding a copied `*Def` and bypassing bindings.

Recommended scope:

- Add a first-class `NodeDefHandle` for the runtime node instance's authored definition.
- Make loaded node definitions resolvable as slot roots by handle.
- Change unbound `ConsumedSlot` fallback to read the node's authored definition slot root.
- Add a small read-only `SlotView` pattern that gives node code typed access while still going through `TickContext` / `EngineSession`.
- Port one real node, likely `TextureNode`, to prove the pattern without trying to convert every node in one pass.
- Keep inline node definitions, editable mutation messages, and full generated `SlotView` codegen as future work.

This is ambitious enough to prove the architecture, but bounded enough to avoid rebuilding shader compile lifecycle, fixture mapping, project sync, and UI in the same step.

## Current Codebase State

Current local changes already made before this plan is finalized:

- `ArtifactManager` has been renamed to `ArtifactStore`, but it is still
  generic as `ArtifactStore<A>`.
- `node_def.rs` has moved from `lpc-model/src/node/` to
  `lpc-model/src/nodes/`, which is the right home for the canonical core
  node-definition vocabulary.
- `lpc-model/src/nodes/node_def.rs` still contains the older `NodeDef` trait.
  This plan should replace that concept with the enum unless a small support
  trait remains clearly useful.

### Resolver

Relevant files:

- `lp-core/lpc-engine/src/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/resolver/query_key.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

`EngineSession` already has the key behavior we want:

- `QueryKey::ConsumedSlot` first checks `BindingRegistry` for a target binding.
- If a binding exists, the session resolves the binding source.
- If no binding exists, the session delegates to `host.produce(query, session)`.

The current host fallback for unbound `ConsumedSlot` is wrong for config:

- `EngineResolveHost::produce(QueryKey::ConsumedSlot { .. })` ticks the same node.
- It then reads that node's `runtime_state_slots()`.
- That means unbound consumed slots are currently treated as runtime-produced state, not authored defaults.

Desired behavior:

- `ProducedSlot` reads runtime state slots.
- `ConsumedSlot` first resolves bindings, then falls back to the node's authored `NodeDef` slot root.

### Runtime Nodes

Relevant files:

- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/nodes/texture/texture_node.rs`

`NodeRuntime` currently exposes:

- `tick(&mut self, TickContext)`
- `runtime_state_slots(&self) -> &dyn SlotAccess`
- runtime state shape registration
- optional render capability

`TickContext` exposes generic `resolve(QueryKey)`, but there is no typed config access helper yet.

`TextureNode` is a good first slice:

- It currently stores `config: TextureDef`.
- On tick it copies `config.width()` / `config.height()` into `TextureState`.
- A small `TextureDefView` could resolve `size` through `QueryKey::ConsumedSlot { node: ctx.node_id(), slot: "size" }`, then `TextureNode` can stop owning `TextureDef`.

`ShaderNode` and `FixtureNode` are larger:

- `ShaderNode` has shader source and compile lifecycle concerns, especially because render may happen after tick.
- `FixtureNode` currently has a wide constructor and mapping/output/buffer behavior; it is important, but too large for the first proof.

### Authored Definitions

Relevant files:

- `lp-core/lpc-engine/src/project_runtime/project_loader.rs`
- `lp-core/lpc-engine/src/project_runtime/source_authoring_index.rs`
- `lp-core/lpc-model/src/nodes/*`

Node defs now live in `lpc-model` and derive slot roots:

- `ProjectDef`
- `TextureDef`
- `ShaderDef`
- `FixtureDef`
- `OutputDef`

`LoadedNodeDef` is currently an engine-local loader enum:

```rust
pub enum LoadedNodeDef {
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}
```

`SourceAuthoringIndex` is currently keyed by runtime `NodeId`:

- `authoring_defs: HashMap<NodeId, LoadedNodeDef>`
- `authoring_paths: HashMap<NodeId, LpPathBuf>`

This works for source sync, but it does not yet model the thing nodes actually need: a durable handle to the authored def.

The codebase already has the closed-set dispatch point for authored node
definitions in `project_loader.rs`:

```rust
match probe.kind.as_str() {
    "texture" => parse_node_def(path, &text).map(LoadedNodeDef::Texture),
    "shader" => parse_node_def(path, &text).map(LoadedNodeDef::Shader),
    "output" => parse_node_def(path, &text).map(LoadedNodeDef::Output),
    "fixture" => parse_node_def(path, &text).map(LoadedNodeDef::Fixture),
    ...
}
```

This is useful evidence: the system is already closed over known node kinds.
The enum should move to `lpc-model` as the canonical `NodeDef`, rather than
staying as an engine-local `LoadedNodeDef` plus a separate `NodeDef` trait.

### Node Tree

Relevant files:

- `lp-core/lpc-engine/src/node/node_entry.rs`
- `lp-core/lpc-engine/src/node/node_tree.rs`

`NodeEntry` currently stores:

- `config: NodeInvocation`
- `artifact: ArtifactId`

The `artifact` field is close to what we need, but it only identifies the artifact, not the slot root/path inside the artifact. For future inline defs, the handle should be able to point into an artifact slot tree.

Recommended type:

```rust
pub struct NodeDefHandle {
    pub artifact: ArtifactId,
    pub path: SlotPath,
}
```

`SlotPath::root()` or equivalent empty path means "the artifact root def". Inline node defs later can use a non-empty path.

### Mockup Reference

Relevant crates:

- `lp-core/lpc-slot-mockup`
- `lp-core/lpc-view`
- `lp-core/lpc-model/src/slot`

The mockup already proved:

- Rust-authored structs can expose `SlotAccess`.
- Dynamic shader params can expose slot data.
- Shape-aware snapshots/diffs can move to a client mirror.
- Mutation requests can use expected revisions and typed validation.

M2.7 should not copy the whole mockup, but it should port the relevant architectural idea: authored slot roots are the defaults that the resolver reads when no binding overrides a consumed slot.

## Suggested Design Direction

### 1. Promote A Canonical `NodeDef` Enum

Move the closed node-definition set into `lpc-model`:

```rust
pub enum NodeDef {
    Project(ProjectDef),
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}
```

The enum should become the one place that names core authored node definition
variants. It should provide the common behavior currently scattered across the
engine-local `LoadedNodeDef` enum and the `NodeDef` trait:

- `kind() -> NodeKind`
- `kind_name() -> &'static str` if still useful
- `SlotAccess` delegation
- TOML kind-dispatched parse helper if the dependency direction stays clean

The current `NodeDef` trait should either be renamed to something like
`NodeDefBody` only if still useful, or deleted if the enum fully replaces it.

Decision:

- Use the enum. The system already branches on TOML `kind`, and we want one
  obvious place to add a core node definition type.
- Do not introduce a dyn-trait artifact payload for this pass.

### 2. Make `ArtifactStore` Own `NodeDef`

`ArtifactStore` should stop being exposed as generic `ArtifactStore<A>` for the
engine domain. It should own LightPlayer authored artifacts directly:

```rust
pub enum ArtifactState {
    Resolved,
    Loaded(NodeDef),
    Prepared(NodeDef),
    Idle(NodeDef),
    ...
}
```

Every loaded artifact in this slice is an authored node definition, including
the project artifact. If future non-node artifacts appear, that is the point to
introduce a separate `LoadedArtifact` enum.

### 3. Add `NodeDefHandle`

Add `NodeDefHandle` in engine node code because it currently depends on `ArtifactId`.

Suggested home:

- `lp-core/lpc-engine/src/node/node_def_handle.rs`

Suggested semantics:

- Identifies the slot root/subtree that defines a runtime node instance.
- `artifact` identifies the loaded authored artifact.
- `path` identifies a slot subtree inside that artifact; root path means the artifact itself.
- For this milestone, only root paths are supported.

### 4. Resolve Node Def Handles Through `ArtifactStore`

The resolver host lives inside `Engine`, and `Engine` already owns the artifact
store. Consumed-slot default lookup should resolve:

```text
NodeId -> NodeEntry::def_handle -> ArtifactStore entry -> NodeDef SlotAccess root/subtree
```

Root-only handle support is enough for now.

`SourceAuthoringIndex` can shrink to debug/path lookup, become a compatibility
wrapper, or be deleted if no longer needed.

### 5. Correct Consumed Slot Fallback

Change `EngineResolveHost::produce(QueryKey::ConsumedSlot { node, slot })`:

- Do not tick the node.
- Read the node's `NodeDefHandle`.
- Look up the authored def slot root.
- Use `lookup_slot_data(def_root, slot_shapes, slot)`.
- Require the result to be `SlotDataAccess::Value`.
- Convert `LpValue` to `RuntimeProduct` using the existing bridge.
- Return `ProductionSource::Default`.

This gives the right model:

- Binding overrides are dynamic and demand-driven.
- Authored def slots are defaults.
- Produced slots remain runtime state.

### 6. Add A Small Manual `SlotView`

Do not build derive/codegen yet.

Add enough real API to make node code clean:

- A generic helper on `TickContext`, such as `resolve_slot_value<T>(&mut self, slot: &SlotPath)`.
- Or a small `SlotView<'ctx>` wrapper that knows `node_id` and delegates to `TickContext::resolve`.
- A manual `TextureDefView` proving how generated views should feel later.

Important:

- Views are read-only.
- Views do not bypass the resolver.
- Views should preserve slot/value conversion errors in a developer-friendly `NodeError`.

### 7. Port `TextureNode` As The First Real Node

Target behavior:

- `TextureNode::new(node_id)` no longer owns `TextureDef`.
- `TextureNode::tick()` reads `size` through the view/resolver.
- Its `TextureState` remains the produced runtime state root.
- Existing texture metadata tests should still pass.
- Add a binding override test proving `TextureDef.size` can be overridden by a binding.

## Open Questions

### Q1. Should `NodeDefHandle` replace `NodeEntry.artifact` immediately?

Context:

- `NodeEntry.artifact` is used for `TickContext::artifact_ref()` and artifact content revision.
- `NodeDefHandle` would contain the same artifact id plus a path.

Suggested answer:

- Add `def_handle: NodeDefHandle` now and keep `artifact` briefly if that keeps churn reasonable.
- If the implementation stays simple, replace `artifact` with `def_handle` and use `def_handle.artifact()` where needed.
- Avoid a long-lived duplicate if it starts to feel confusing.

### Q2. Should authored defs be owned by `Engine`?

Context:

- `EngineResolveHost` needs authored defaults during `ConsumedSlot` fallback.
- `SourceAuthoringIndex` currently lives on `CoreProjectRuntime`, outside `Engine`.

Suggested answer:

- Yes, move the authoring store into `Engine` or make `Engine` own the canonical one.
- `CoreProjectRuntime` can expose delegated accessors.
- Resolver config access should not require a project-runtime wrapper.

Decision:

- Yes, but not as a separate authored-def store. The engine's `ArtifactStore`
  should own loaded `NodeDef` payloads.
- A later cleanup pass may revisit broader ownership/storage boundaries, but
  this is the right direction for the current runtime architecture.

### Q2b. Should loaded node definitions use an enum or dyn trait?

Context:

- The loader already hard-codes a `kind` dispatch into an engine-local
  `LoadedNodeDef` enum.
- The current `NodeDef` trait does not provide deserialization by itself; a
  registry or enum is required somewhere.
- We want adding a core node type to require touching one obvious place.

Suggested answer:

- Use a canonical `NodeDef` enum in `lpc-model`.
- Include `Project(ProjectDef)` because the project artifact defines the root
  project node.
- Let the enum implement/delegate common behavior like `kind()` and
  `SlotAccess`.
- Keep plugin/dynamic node-kind support as future work.

Decision:

- Use the canonical enum approach.

### Q3. How much `SlotView` should M2.7 build?

Context:

- The mockup proves generic dynamic access, but real ergonomic typed views are not ported.
- Full codegen is likely its own milestone.

Suggested answer:

- Build the minimal hand-written version only:
  - generic typed `TickContext` helper
  - one manual `TextureDefView`
  - tests that make the desired generated API obvious

### Q4. Which real node should be ported first?

Context:

- `TextureNode` is small and already copies `TextureDef` into `TextureState`.
- `ShaderNode` and `FixtureNode` have larger lifecycle problems.

Suggested answer:

- Port `TextureNode` first.
- Leave shader/fixture migration for follow-up, but record the intended next steps.

## User Notes To Preserve

- Nodes should not be given direct access to their defs because that bypasses the resolver and bindings.
- `SlotView` is read-only.
- Nodes should not write to their definitions; future writes should go through message/mutation APIs.
- A node definition handle should support artifact root now and inline defs later.
- Inline defs are not needed yet, but the handle shape should leave room for them.
- The mockup is the reference for the desired architecture, but this milestone should port only the useful slice into the real engine.

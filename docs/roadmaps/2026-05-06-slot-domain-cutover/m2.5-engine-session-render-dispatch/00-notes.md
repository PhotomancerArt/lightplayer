# M2.5 Engine Session Render Dispatch Notes

## Scope

This milestone cleans up the M2.4 render-flow spike so render products become handles back to node-owned rendering, not owners of shader compilation/render state.

In scope:

- Introduce the `EngineSession` vocabulary as the general runtime request/session concept.
- Keep slot resolution working through the existing resolver machinery while moving non-slot render materialization out of `TickResolver`.
- Add a node render capability (`RenderNode` or equivalent) instead of putting render methods on every node.
- Replace `ShaderRenderProduct` as a stateful renderer with a node-owned render product handle.
- Move shader compilation, compiled shader cache, render implementation, and compile error state back into `ShaderNode`.
- Add `NodeEntryState::Executing` so the tree can represent an active node call explicitly instead of temporarily disguising it as `Pending`.
- Disallow node re-entry through the session for now, with clear dev-facing errors.

Out of scope:

- Real async/await or a general executor.
- Allowing same-node re-entry.
- Control/DMX products from fixture to output.
- Generic mutation sessions or client-driven mutation.
- Rebuilding the wire/UI sync surfaces.

## Current State

### Resolver/session

Relevant files:

- `lp-core/lpc-engine/src/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/resolver/tick_resolver.rs`
- `lp-core/lpc-engine/src/resolver/resolve_host.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

`ResolveSession` already provides the seed of a runtime scheduler:

- per-frame session state,
- resolution cache,
- active query stack and cycle detection,
- tracing,
- binding selection,
- callback into a host for uncached produced/consumed slot production.

But it is still slot/query-centric. M2.4 added `render_texture` to `TickResolver` and `ResolveHost`, which makes the resolver feel responsible for render services. That is the wrong boundary.

Suggested direction: introduce an `EngineSession` layer around or above `ResolveSession`. `ResolveSession` may remain the slot resolver implementation initially, but node-facing contexts should talk to a general session/dispatcher for non-slot work.

Updated decision: avoid two overlapping execution sessions. `ResolveSession` should evolve into `EngineSession` in this milestone if practical. Slot resolution remains one service on the general session. The lower-level `Resolver` may stay as the cache/materialization helper, but it should not be the node-calling execution session.

The decoupling benefit of `ResolveSession` should be preserved through a host trait and fake test hosts, not through a second session abstraction:

```rust
trait EngineSessionHost {
    fn produce_slot(...);
    fn render_product(...);
    fn runtime_buffer_mut(...);
}
```

Tests can construct `EngineSession` with a fake `EngineSessionHost` without requiring a full `Engine`.

### Node execution

Relevant files:

- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/node/node_entry_state.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

Current node execution temporarily removes a node payload from the tree by replacing `NodeEntryState::Alive(node)` with `NodeEntryState::Pending`, calls `tick`, then restores `Alive(node)`.

This is safe Rust, but the state is semantically wrong:

- `Pending` means not instantiated, not executing.
- A node being executed should have its own state.
- Re-entry currently fails as “not alive” rather than “node is already executing”.

Suggested direction: add `NodeEntryState::Executing { call: NodeCallKey }`. Keep the safe take/restore implementation for now, but make the intermediate state truthful and diagnostics explicit.

### Render products

Relevant files:

- `lp-core/lpc-engine/src/render_product/render_product_store.rs`
- `lp-core/lpc-engine/src/render_product/render_texture_request.rs`
- `lp-core/lpc-engine/src/render_product/texture_product.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_render_product.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`

Current M2.4 state:

- `RenderProduct` is a trait object in `RenderProductStore`.
- `ShaderRenderProduct` owns shader config, GLSL source, compiled shader, compile error state, and render implementation.
- `ShaderNode` clones config/source into `ShaderRenderProduct`, then mainly publishes the render product id.
- `FixtureNode` calls `ctx.render_texture`.

This works but puts ownership in the wrong place:

- Compile errors naturally belong to `ShaderNode` state.
- Shader source/config are duplicated between node and product.
- Render products are acting like renderers rather than handles.
- The resolver now has render-specific APIs.

Decision: a render product should still be called `RenderProduct` in the runtime product vocabulary, because through the dataflow graph it is the thing produced by a node. Whether it is materialized is an implementation detail.

However, it should be a small value handle, not a registry-owned renderer. For the current system that likely means:

```rust
pub struct RenderProduct {
    pub node: NodeId,
    pub output: u32,
}
```

The `output` field is a compact node-local render output id. Do not use `SlotPath` here: the render product is already being returned from a produced slot, and render output identity should not be coupled to slot addressing. The engine session uses this handle to call the owning node’s render capability.

This implies `RenderProductStore` can likely be deleted from the node render path entirely. A future concrete `TextureRegistry` may own materialized textures, but a node render product is just a callable product value.

### Capability model

The desired shape is capability-based:

```rust
trait NodeRuntime {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError>;

    fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
        None
    }
}

trait RenderNode {
    fn render_texture(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError>;
}
```

Only render-capable nodes override `render_node`.

When a node produces a `RuntimeProduct::Render`, the engine should be able to validate that the owning node exposes the render capability. If not, fail with a clear dev-focused error like:

`node <id> cannot produce a render product: NodeRuntime::render_node() returned None`

This is explicit and good enough until macro/codegen support exists.

### Produced slot access

Current nodes often use sidecar structs such as `ShaderProducedSlots` and then implement:

```rust
fn produced(&self) -> &dyn ProducedSlotAccess {
    &self.outputs
}
```

That is boilerplate-heavy and obscures the simple case where a node itself owns and exposes its produced slots.

Suggested direction: let nodes implement `ProducedSlotAccess` directly when that is natural:

```rust
impl ProducedSlotAccess for ShaderNode {
    fn get(&self, path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
        ...
    }
}

impl NodeRuntime for ShaderNode {
    fn produced(&self) -> &dyn ProducedSlotAccess {
        self
    }
}
```

Sidecar access structs are still fine when they remove real complexity, but they should not be the default pattern.

## User Decisions And Guidance

- Render products should be value handles back to node-owned rendering, not renderers that own shader state.
- Keep the name `RenderProduct`, not `RenderRef`; graph-wise it is the product, even if implementation-wise it is a handle.
- A render product should carry node id and a compact node-local render output id, not a `SlotPath`.
- Delete `RenderProductStore` from the core node-render flow if possible.
- `ShaderNode` should own the shader, compile state, compilation errors, and render implementation.
- A node should not re-enter itself through the session. That is a smell and not needed now.
- If a node needs to call its own render helper, it can do so directly on `self`; cross-node work goes through the session/dispatcher.
- `NodeEntryState::Executing` is the right semantic state while a node is actively being called.
- `EngineSession` feels like the right name for the generalized session.
- Prefer one `EngineSession` over `EngineSession` plus `ResolveSession`; avoid duplicate stack/host/session concepts.
- Keep `Resolver` only as lower-level cache/materialization machinery if useful.
- Prefer implementing `ProducedSlotAccess` directly on nodes for simple produced slots.
- Future work may introduce `ControlProduct` / `DmxProduct` for fixture-to-output wiring, but not in this cleanup.

## Fundamental Vocabulary

Proposed vocabulary:

- `EngineSession`: one active runtime execution conversation for a frame/request. It owns request caches, active stacks, traces, and typed request APIs.
- `EngineRequest`: a top-level request handled by the session, such as resolve slot or render product.
- `EngineRequestKey`: hashable identity used for cache/cycle detection.
- `NodeCall`: a request targeted at a specific node, such as tick, produce slot, or render product.
- `NodeCallKey`: identity for an active node call, stored in `NodeEntryState::Executing`.
- `RenderNode`: node capability trait for nodes that can render products.
- `RenderProduct`: a small runtime product value that identifies an owning node and node-local render output id. It is a graph product, not a materialized texture and not a renderer object.

## Open Questions

### Q1: Should `ResolveSession` be renamed now or wrapped first?

Context: `ResolveSession` already contains a lot of tested slot-resolution behavior. A wholesale rename to `EngineSession` could be large but may reduce confusion immediately.

Decision: prefer one session. Rename/evolve `ResolveSession` into `EngineSession` in this milestone if the change remains manageable. Do not create a long-lived wrapper stack of `EngineSession -> ResolveSession -> ResolveHost`.

### Q2: Should `RenderProductStore` be removed from the node render path?

Context: if render products are just node-owned callable graph products, a registry keyed by `RenderProductId` is unnecessary indirection. The produced slot can return the render product value directly. Concrete materialized textures are a different concept and likely belong in a future `TextureRegistry`.

Suggested answer: yes. Remove `RenderProductStore` from the shader-to-fixture path and replace `RuntimeProduct::Render(RenderProductId)` with `RuntimeProduct::Render(RenderProduct)`. If old texture-backed test doubles become awkward, rewrite those tests around a tiny render-capable test node rather than keeping the registry alive.

### Q3: How strict should capability validation be?

Context: without a render product registry, capability validation shifts from “registration time” to “production/render time.” We still want mistakes to fail clearly and early when possible.

Suggested answer: validate in the engine session when rendering a product and, if practical, when reading a produced slot containing a render product. The error should name the owner node and explain that the node did not expose `render_node()`.

### Q4: Do render requests need cycle detection now?

Context: cross-node render calls can recurse through slot resolution. Same-node re-entry should be rejected. Exact render cycles are unlikely in the MVP but possible once render calls can resolve inputs.

Suggested answer: add minimal active `NodeCallKey` tracking for node calls now. It should catch node executing/re-entry cases. Full render request caching/cycle detection can be extended later if needed.

### Q5: Should fixture output become a product now?

Context: the user noted that a future `ControlProduct`/`DmxProduct` might cleanly wire fixture to output. Current fixture still writes directly to output buffers.

Suggested answer: no. Record in `future.md`. This milestone only fixes shader render-product ownership and session boundaries.

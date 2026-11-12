# M2.5 Engine Session Render Dispatch Design

## Scope

This milestone replaces the M2.4 render-product spike with a cleaner node-owned render model.

The goal is to make the engine execution vocabulary line up with the domain:

- one `EngineSession` handles runtime requests,
- render products are graph products produced by nodes,
- shader nodes own shader compile/render state,
- render materialization dispatches back to a render-capable node,
- active node calls are represented truthfully as `Executing`.

This plan should not introduce async/await, same-node re-entry, a texture registry, fixture-to-output products, or client mutation work.

## File Structure

```text
lp-core/lpc-engine/src/
  engine/
    engine.rs
    engine_error.rs
    engine_session.rs          # new or renamed from resolver/resolve_session.rs
    engine_session_host.rs     # new or renamed from resolver/resolve_host.rs
  node/
    contexts.rs
    node_call.rs               # new: NodeCall / NodeCallKey vocabulary
    node_entry_state.rs
    node_runtime.rs
    render_node.rs             # new: RenderNode capability
  nodes/
    fixture/fixture_node.rs
    shader/shader_node.rs
    shader/shader_render_product.rs # deleted or reduced away
  render_product/
    render_product.rs          # new: small value product, if split from runtime_product
    render_product_id.rs       # deleted if no longer used
    render_product_store.rs    # deleted or reduced out of node render flow
    render_texture_request.rs
    texture_product.rs
  resolver/
    resolver.rs                # keep as cache/materialization helper
    resolver_cache.rs
    query_key.rs               # either keep for slot requests or rename later
```

The exact final filenames can adjust during implementation, but keep one primary concept per file.

## Architecture Summary

`EngineSession` is the single active execution session. It owns request caches, active stacks, tracing, and typed methods for runtime work:

```rust
session.resolve(query)
session.render_texture(product, request)
session.runtime_buffer_mut(...)
```

Slot resolution remains part of the session. The existing `Resolver` can remain as a lower-level cache/helper, but `ResolveSession` should not continue as a separate node-calling session.

Nodes expose universal lifecycle through `NodeRuntime`. Render-capable nodes additionally expose `RenderNode`:

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

`RenderProduct` is a value in the runtime graph:

```rust
pub struct RenderProduct {
    pub node: NodeId,
    pub output: u32,
}
```

`RuntimeProduct::Render(RenderProduct)` replaces `RuntimeProduct::Render(RenderProductId)`. There is no render product registry in the node render path. Future materialized texture resources should use a separate `TextureRegistry` concept.

`ShaderNode` owns:

- `ShaderDef`,
- GLSL source,
- compiled shader cache,
- compilation error state,
- render implementation.

`ShaderNode` produces `RuntimeProduct::Render(RenderProduct { node: self.node_id, output: 0 })` from its `output` slot. It implements `RenderNode` and renders that product when the engine session dispatches a render request to it.

`FixtureNode` resolves its `input` slot and receives a `RuntimeProduct::Render(RenderProduct)`. It then asks the session/context to materialize the product as a texture. The session dispatches the render request to the owning node.

## Execution Semantics

Node calls are tracked explicitly:

```rust
enum NodeCall {
    Tick,
    ProduceSlot { slot: SlotPath },
    Render { product: RenderProduct },
}

struct NodeCallKey {
    node: NodeId,
    call: NodeCall,
}
```

`NodeEntryState::Executing { call: NodeCallKey }` replaces the current temporary use of `Pending` while a node payload is moved out and called.

Same-node re-entry through the session is not supported. If a node is already `Executing`, any attempt to call it through the session should return a clear error naming the active call and attempted call. If a node wants to reuse its own behavior, it should call private helper methods on `self`.

Cross-node calls are allowed and remain the core demand-driven behavior.

## Produced Slot Access

For simple nodes, prefer implementing `ProducedSlotAccess` directly on the node:

```rust
impl ProducedSlotAccess for ShaderNode { ... }

impl NodeRuntime for ShaderNode {
    fn produced(&self) -> &dyn ProducedSlotAccess {
        self
    }
}
```

Sidecar produced-slot structs are still allowed when they reduce complexity, but should not be the default for node-owned outputs.

## Main Interactions

Shader-to-fixture flow:

```text
FixtureNode::tick
  ctx.resolve(fixture.input)
    EngineSession resolves binding bus#visual.out
    EngineSession produces ShaderNode.output
      Engine dispatches ShaderNode tick if needed
      ShaderNode produced access returns RenderProduct { node: shader_id, output: 0 }
  ctx.render_texture(product, request)
    EngineSession dispatches RenderNode::render_texture to shader_id
    ShaderNode compiles lazily if needed
    ShaderNode renders full texture
  FixtureNode maps texture and writes output buffer
```

Capability failure:

```text
ctx.render_texture(RenderProduct { node: non_render_node, output: 0 }, request)
  -> EngineSession looks up node
  -> node.render_node() returns None
  -> error: node <id> cannot render product output 0: NodeRuntime::render_node() returned None
```

Re-entry failure:

```text
ShaderNode is Executing { call: Render { ... } }
session attempts to call ShaderNode again
  -> error: node <id> is already executing Render; re-entry through EngineSession is unsupported
```


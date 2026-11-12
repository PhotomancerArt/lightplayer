# Phase 3: Resolver Publication And Render Delegation Primitives

- parallel: -
- sub-agent: supervised

## Scope Of Phase

Add two engine primitives needed by playlist runtime:

- early publication of produced runtime slots into the current resolver cache;
- visual render delegation callbacks from `RenderContext`.

In scope:

- `TickResolver::publish_produced_slot`;
- `TickContext::publish_runtime_slot`;
- `EngineSession` cache insertion API;
- `RenderContext` child visual render services;
- tests for cache publication and render delegation.

Out of scope:

- Playlist state machine.
- Playlist crossfade blending.
- Loader recursion.

## Code Organization Reminders

- Keep the resolver primitive generic. Do not name playlist in resolver APIs.
- Reuse `Production`, `QueryKey::ProducedSlot`, `lookup_slot_data_and_shape`, and
  `lpc_wire::snapshot_slot_shape`.
- Mirror the existing `ControlRenderContext` service pattern for render delegation.
- Keep helper comments concise and focused on why early publication exists.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Resolver files:

```text
lp-core/lpc-engine/src/dataflow/resolver/resolve_session.rs
lp-core/lpc-engine/src/dataflow/resolver/tick_resolver.rs
lp-core/lpc-engine/src/node/contexts.rs
```

Add an `EngineSession` method:

```rust
pub fn publish(&mut self, query: QueryKey, production: Production)
```

or a narrower method:

```rust
pub fn publish_produced_slot(&mut self, node: NodeId, slot: SlotPath, production: Production)
```

Then add to `TickResolver`:

```rust
fn publish_produced_slot(
    &mut self,
    node: NodeId,
    slot: SlotPath,
    production: Production,
) -> Result<(), ResolveError>;
```

`SessionHostResolver` should insert into the active session cache. The resolver already checks cache
before host production, so no special-case branch should be needed.

Add to `TickContext`:

```rust
pub fn publish_runtime_slot(
    &mut self,
    state_root: &dyn SlotAccess,
    slot: SlotPath,
) -> Result<(), NodeError>
```

This method should:

1. look up data and shape from `state_root`;
2. snapshot the slot data with the current `SlotShapeRegistry`;
3. create `ProductionSource::ProducedSlot { node: self.node_id, slot: slot.clone() }`;
4. publish it through `TickResolver`.

Render files:

```text
lp-core/lpc-engine/src/node/contexts.rs
lp-core/lpc-engine/src/node/render_node.rs
lp-core/lpc-engine/src/engine/engine.rs
```

Add a trait analogous to `ControlRenderServices`:

```rust
pub trait VisualRenderServices {
    fn render_texture(...);
    fn render_texture_into(...);
    fn sample_visual_into(...);
}
```

Then expose these through `RenderContext`:

```rust
ctx.render_texture_into(child_product, request, target)
ctx.sample_visual_into(child_product, request, target)
```

`EngineResolveHost` already has `render_node_texture`, `render_node_texture_into`, and
`sample_node_visual_into`; wire those into the new service implementation.

Tests:

- A fake node publishes a produced slot during tick; a downstream node resolves that produced slot
  without invoking host production again.
- Publishing replaces or satisfies the same frame cache for the exact `ProducedSlot` query only.
- A test render node can delegate to another render node through `RenderContext`.
- Re-entry guard still rejects rendering the same visual product recursively.

## Validate

Run:

```bash
cargo test -p lpc-engine resolver
cargo test -p lpc-engine contexts
cargo check -p lpc-engine
```

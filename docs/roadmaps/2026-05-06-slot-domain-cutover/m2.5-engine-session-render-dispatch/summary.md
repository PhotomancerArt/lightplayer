# M2.5 Summary

## What was built

- Added `EngineSession` as the engine-facing session name while preserving `ResolveSession` as a transitional alias.
- Replaced shader-to-fixture `RuntimeProduct::Render(RenderProductId)` flow with node-owned `RuntimeProduct::Render(RenderProduct { node, output })`.
- Added `NodeCall`, `NodeCallKey`, `NodeEntryState::Executing`, and the optional `RenderNode` capability.
- Moved shader compilation/rendering ownership back into `ShaderNode` and deleted `ShaderRenderProduct`.
- Routed fixture render materialization through engine dispatch back to the render-capable producing node.
- Removed render-store sampling/materialization from `TickContext` and `ResolveHost`; `RenderProductStore` remains only as a store-backed legacy/resource-projection helper.
- Fixed a flaky semantic-slot revision test so it no longer assumes no other test touches the ambient revision concurrently.

## Decisions for future reference

#### Render Products Are Graph Values

- **Decision:** `RenderProduct` is a small node-owned graph value, not a store entry.
- **Why:** The node owns shader compile state and render errors; the product just identifies which node/output can materialize the visual.
- **Rejected alternatives:** render product registry for shader output; stateful `ShaderRenderProduct` owning compilation.
- **Revisit when:** texture-backed producer-owned resources are introduced.

#### Render Capability Is Explicit

- **Decision:** render-capable nodes opt in with `NodeRuntime::render_node() -> Option<&mut dyn RenderNode>`.
- **Why:** Most nodes cannot render, and capability failure should be a clear engine error.
- **Rejected alternatives:** `render_texture` on every node; dynamic downcast/type checks.
- **Revisit when:** derive/macros or typed node registration can remove the manual opt-in without hiding the capability.

#### Same-Node Reentry Is Unsupported

- **Decision:** active node calls set `NodeEntryState::Executing` and reject re-entry through the engine session.
- **Why:** Demand-driven node work should produce products that may later be materialized; recursive calls back into the same node are a design smell for this engine slice.
- **Rejected alternatives:** allowing unchecked re-entry; continuing to use `Pending` as a temporary stolen-node state.
- **Revisit when:** a real scheduler introduces resumable/async-like execution.

#### Store-Backed Render Products Are Quarantined

- **Decision:** the old store-backed trait is now `StoredRenderProduct`; it remains for resource projection and tests, not core node rendering.
- **Why:** Keeping it separate avoids conflating materialized texture storage with dataflow render products.
- **Rejected alternatives:** deleting the store immediately; continuing to expose store rendering through `TickContext`.
- **Revisit when:** the resource sync cleanup introduces a real texture registry.

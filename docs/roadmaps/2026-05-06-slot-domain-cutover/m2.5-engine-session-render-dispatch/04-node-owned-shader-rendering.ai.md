# Phase 4: Node-Owned Shader Rendering

## Scope Of Phase

Move shader compilation/rendering ownership back into `ShaderNode` and dispatch render requests through the render capability.

In scope:

- Delete or retire stateful `ShaderRenderProduct`.
- Move compiled shader cache and compilation error state back into `ShaderNode`.
- Implement `RenderNode` for `ShaderNode`.
- Make `ShaderNode` produce `RuntimeProduct::Render(RenderProduct { node: self.node_id, output: 0 })`.
- Make fixture render materialization call through `EngineSession`, not `TickResolver` render APIs.
- Remove render-specific methods from `TickResolver` and old host traits.

Out of scope:

- Shader params/bindings beyond what already exists.
- Texture registry.
- Fixture-to-output products.
- Same-node re-entry.

## Code Organization Reminders

- Keep shader compile helpers near `ShaderNode` if they are shader-node implementation details.
- Use one concept per file; delete `shader_render_product.rs` if it no longer has a real concept.
- Keep compile error state clearly named on `ShaderNode` so future slot-state exposure can find it.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_render_product.rs`
- `lp-core/lpc-engine/src/nodes/shader/mod.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/resolver/tick_resolver.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/render_product/render_product_store.rs`

Expected changes:

- `ShaderNode` fields should include the source/config once, plus:

```rust
shader: Option<Box<dyn LpShader>>,
compilation_error: Option<String>,
```

- Add a private helper such as:

```rust
fn render_texture_impl(
    &mut self,
    request: &RenderTextureRequest,
    ctx: &mut RenderContext<'_>,
) -> Result<TextureRenderProduct, NodeError>
```

- `RenderNode for ShaderNode` should call that helper.
- `NodeRuntime for ShaderNode` should implement:

```rust
fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
    Some(self)
}
```

- `FixtureNode::tick` should call `ctx.render_texture(product, request)` where `product` is the `RenderProduct` value resolved from its input.
- `ctx.render_texture` should dispatch through `EngineSession`, not through `TickResolver`.
- Remove `RenderProductStore::render_texture` and `RenderProduct::render_texture` from the main path.

Validation focus:

- Shader tests should prove produced output is a render product value owned by the shader node.
- Shader render test should prove lazy compile/render still works and sample color is correct.
- Fixture tests should prove fixture consumes the render product and writes output buffer.
- Add or update a test proving a non-render node render request gives a clear capability error.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine nodes::shader
cargo test -p lpc-engine nodes::fixture
cargo test -p lpc-engine project_runtime::core_project_runtime
```


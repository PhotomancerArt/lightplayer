# Phase 2: Render Product Value

## Scope Of Phase

Replace registry-backed render product identity with a small runtime product value.

In scope:

- Introduce `RenderProduct` as a value handle containing `node: NodeId` and `output: u32`.
- Change `RuntimeProduct::Render(RenderProductId)` to `RuntimeProduct::Render(RenderProduct)`.
- Remove `RenderProductId` from shader-to-fixture dataflow.
- Update produced-slot access and tests that inspect render products.
- Keep `TextureRenderProduct` as the materialized texture response type.

Out of scope:

- Removing every texture-backed test helper if it causes churn; rewrite tests as needed, but keep phase focused.
- Moving shader compile/render state back into `ShaderNode`; that is Phase 4.
- Introducing `TextureRegistry`.

## Code Organization Reminders

- Prefer `render_product/render_product.rs` for the value type if it deserves its own file.
- Do not overload `TextureRenderProduct`; it is a materialized texture, not graph product identity.
- Keep names explicit: `RenderProduct` for graph product, `TextureRenderProduct` for materialized texture data.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/render_product/mod.rs`
- `lp-core/lpc-engine/src/render_product/render_product_id.rs`
- `lp-core/lpc-engine/src/render_product/render_product_store.rs`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`

Expected changes:

- Add:

```rust
pub struct RenderProduct {
    pub node: NodeId,
    pub output: u32,
}
```

- Change helpers like `RuntimeProduct::render(...)` to take/return `RenderProduct`.
- Update `as_render()` return type.
- `ShaderNode` should eventually produce `RenderProduct { node: self.node_id, output: 0 }`; this phase can update types before moving all shader render state.
- Remove or quarantine `RenderProductId` usage in the core flow.
- If `RenderProductStore` still exists after this phase, it should no longer be needed for shader-to-fixture rendering. Mark remaining uses for later deletion in Phase 4/5.

Validation focus:

- Runtime product tests should demonstrate render product value round trips.
- Fixture/shader tests may need temporary updates, but should stay meaningful.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine runtime_product::
cargo test -p lpc-engine render_product::
cargo test -p lpc-engine nodes::shader --no-run
cargo test -p lpc-engine nodes::fixture --no-run
```


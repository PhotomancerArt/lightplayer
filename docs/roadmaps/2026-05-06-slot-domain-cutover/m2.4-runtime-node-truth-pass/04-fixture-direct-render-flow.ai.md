# Phase 4: Fixture Direct Render Flow

## Scope Of Phase

Make `FixtureNode` consume shader render products directly and own transient full-texture materialization for mapping.

In scope:

- Remove `shader_node_id` plus `texture_node_id` coupling from fixture runtime logic.
- Make fixture resolve its consumed `input` slot.
- Expect `RuntimeProduct::Render`.
- Request full-texture rendering through `TickContext`.
- Feed the returned `TextureRenderProduct` into existing mapping/accumulation logic.
- Preserve output sink writes and lamp-color buffer writes.

Out of scope:

- Loader/example changes except local test scaffolding needed for fixture tests.
- Output node redesign.
- Texture-node many-to-many support.

## Code Organization Reminders

- Keep mapping math in existing fixture mapping modules.
- If helper conversion from `TextureRenderProduct` to accumulation input is needed, keep it near existing sampling/materialization helpers.
- Avoid broad fixture refactors unrelated to the flow change.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/render_product/texture_product.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- fixture tests in `fixture_node.rs`
- project runtime output sink tests in `core_project_runtime.rs`

Expected fixture behavior:

- `FixtureNode::tick()` resolves:

```rust
QueryKey::ConsumedSlot {
    node: ctx.node_id(),
    slot: SlotPath::parse(\"input\")?,
}
```

- The resolved product must be `RuntimeProduct::Render(id)`.
- Fixture constructs a `RenderTextureRequest`.
- MVP dimensions should come from the fixture/runtime config available after this phase. If texture dimensions are still needed during transition, pass them as fixture render target config rather than resolving a texture node.
- Fixture calls the new context render method and receives a `TextureRenderProduct`.
- Fixture uses native texture accumulation path when format is compatible; otherwise use generic sampling from the returned texture product.

Tests to add/update:

- fixture consumes its `input` binding from a shader-like render product.
- fixture no longer needs `shader_node_id`.
- fixture no longer resolves texture width/height produced slots.
- output sink flush tests still pass.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine nodes::fixture
cargo test -p lpc-engine project_runtime::core_project_runtime::output_sink_flush_tests
```

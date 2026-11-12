# Phase 1: Visual Naming Cleanup

## Scope Of Phase

Finish the `RenderProduct` -> `VisualProduct` cleanup already started in the
worktree.

In scope:

- Rename graph-level product types, value variants, slots, resource domains, and
  docs from render-product terminology to visual-product terminology.
- Keep operation names such as `render_texture` when they describe an action.
- Keep concrete materialized texture names if they are accurate, such as
  `TextureRenderProduct`, unless a local rename is obviously clearer.

Out of scope:

- Adding `ControlProduct`.
- Changing fixture/output behavior.
- Large wire/view redesign beyond compile-required naming updates.

## Code Organization Reminders

- Prefer one concept per file.
- Use search-friendly filenames.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

Relevant files and symbols:

- `lp-core/lpc-model/src/resource/visual_product.rs`
- `lp-core/lpc-model/src/resource/visual_product_id.rs`
- `lp-core/lpc-model/src/value/lp_value.rs`
- `lp-core/lpc-model/src/slots/render_product.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/node/render_node.rs`
- `lp-core/lpc-engine/src/node/node_call.rs`
- `lp-core/lpc-engine/src/visual_product/`
- `lp-core/lpc-wire/src/project/resource_sync.rs`
- `lp-core/lpc-view/src/project/resource_cache.rs`

Expected changes:

- Eliminate stale public names like `RenderProductSlot` if possible, replacing
  them with `VisualProductSlot`.
- Route visual products through `LpValue::Product(ProductRef::Visual(_))` if
  current code still uses a product-specific value variant.
- Rename `RuntimeProduct::Render` to `RuntimeProduct::Visual` if current code
  still uses the old variant.
- Keep visual products out of `ResourceDomain`; products are lazy graph values,
  not store-backed resources.
- Update rustdocs and error messages to avoid stale render-product language.

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-engine
```

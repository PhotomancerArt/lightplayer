# Phase 3: Add Minimal RenderProduct Store Wiring

sub-agent: yes
model: kimi-k2.5
parallel: -

## Scope of phase

Add a minimal engine-managed render-product store and prove that a
`RuntimeProduct::Render(RenderProductId)` can be sampled through an engine/store
boundary in tests.

In scope:

- Add `RenderProductStore` and a small product trait or enum suitable for tests.
- Add deterministic fake/test products.
- Add store ownership to `Engine` if needed for the sampling boundary.
- Add tests for registering a product, resolving/carrying a render handle, and
  sampling a batch.

Out of scope:

- Do not implement real shader-backed render products.
- Do not implement texture-backed products.
- Do not add GPU storage.
- Do not add texture wire transport.
- Do not change legacy shader/fixture/output runtimes.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public types and impls near the top; helpers below them.
- Place tests at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]`; fix warnings.
- Do not disable, skip, or weaken existing tests.
- If blocked or ambiguous, stop and report instead of improvising.
- Report back: files changed, validation run, result, and deviations.

## Implementation Details

Create or update:

```text
lp-core/lpc-engine/src/render_product/render_product_store.rs
lp-core/lpc-engine/src/render_product/mod.rs
lp-core/lpc-engine/src/engine/engine.rs
```

The store should be intentionally small. Use `alloc::collections::BTreeMap` or
another no-std-compatible existing pattern from the crate.

One acceptable shape:

```rust
pub trait RenderProduct {
    fn sample_batch(
        &self,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError>;
}

pub struct RenderProductStore {
    next_id: u32,
    products: BTreeMap<RenderProductId, Box<dyn RenderProduct>>,
}
```

If a trait object makes the first implementation awkward, use a private enum or
test-only fixed product shape. The key requirement is that callers sample by
`RenderProductId`, not by owning the product directly.

Add a small error type if needed:

```rust
pub enum RenderProductError {
    UnknownProduct { id: RenderProductId },
    SampleCountMismatch,
}
```

Expose store helpers:

```rust
impl RenderProductStore {
    pub fn new() -> Self;
    pub fn insert(&mut self, product: Box<dyn RenderProduct>) -> RenderProductId;
    pub fn sample_batch(
        &self,
        id: RenderProductId,
        request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError>;
}
```

If added to `Engine`, provide:

```rust
pub fn render_products(&self) -> &RenderProductStore;
pub fn render_products_mut(&mut self) -> &mut RenderProductStore;
```

Test product ideas:

- `SolidColorProduct { color: [f32; 4] }` returns the same color for each point.
- `CoordinateProduct` returns `[x, y, 0.0, 1.0]` for each point.

Suggested tests:

- `store_samples_registered_solid_product`
- `store_errors_for_unknown_product`
- `runtime_product_render_handle_can_be_sampled_via_engine_store`

The final test can manually construct:

```rust
let id = engine.render_products_mut().insert(...);
let product = RuntimeProduct::render(id);
let id = product.as_render().expect("render product");
let result = engine.render_products().sample_batch(id, &request).expect("sample");
```

This proves the core wiring without adding resolver production of render
products yet.

## Validate

Run:

```bash
cargo test -p lpc-engine render_product
```

If that filter misses tests unexpectedly, run:

```bash
cargo test -p lpc-engine
```

# Phase 1: Add RuntimeProduct And RenderProduct Types

sub-agent: yes
model: composer-2
parallel: -

## Scope of phase

Add the new engine-owned domain/product type modules without changing resolver
behavior yet.

In scope:

- Add `lpc-engine/src/runtime_product/` with `RuntimeProduct`.
- Add `lpc-engine/src/render_product/` with lightweight render-product handle
  and sample request/result types.
- Export the new types from `lpc-engine/src/lib.rs`.
- Add focused unit tests for constructors/helpers and sample structs.

Out of scope:

- Do not change `Production` to use `RuntimeProduct` yet.
- Do not add `RenderProductStore` yet.
- Do not remove `ModelValue::Texture2D`.
- Do not change legacy `ResolvedSlot`, `RuntimePropAccess`, or source loading.
- Do not implement real shader/texture rendering.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public types and impls near the top; helpers below them.
- Place tests at the bottom of each module file.
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

Create:

```text
lp-core/lpc-engine/src/runtime_product/
├── mod.rs
└── runtime_product.rs

lp-core/lpc-engine/src/render_product/
├── mod.rs
├── render_product_id.rs
├── sample_request.rs
└── sample_result.rs
```

`RuntimeProduct` should be the future payload/product enum for engine
resolution:

```rust
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProductId),
}
```

Add helpers:

```rust
impl RuntimeProduct {
    pub fn value(value: LpsValueF32) -> Self;
    pub fn render(id: RenderProductId) -> Self;
    pub fn as_value(&self) -> Option<&LpsValueF32>;
    pub fn as_render(&self) -> Option<RenderProductId>;
}
```

Derives should be enough for current tests and cache use. Use `Clone` and
`Debug`. Add `PartialEq` only if useful and supported by fields.

`RenderProductId` should be small, copyable, orderable, and suitable as a map
key:

```rust
pub struct RenderProductId(u32);
```

Add `new(raw: u32) -> Self` and `as_u32(self) -> u32`.

Define first sample structs:

```rust
pub struct RenderSamplePoint {
    pub x: f32,
    pub y: f32,
}

pub struct RenderSampleBatch {
    pub points: Vec<RenderSamplePoint>,
}

pub struct RenderSample {
    pub color: [f32; 4],
}

pub struct RenderSampleBatchResult {
    pub samples: Vec<RenderSample>,
}
```

Use `alloc::vec::Vec`; this crate is `no_std + alloc`.

Export the modules/types from `lpc-engine/src/lib.rs`.

Suggested unit tests:

- `runtime_product_value_helper_returns_value`
- `runtime_product_render_helper_returns_id`
- `render_product_id_round_trips_raw`
- `sample_batch_holds_points_and_results_hold_samples`

## Validate

Run:

```bash
cargo test -p lpc-engine runtime_product
cargo test -p lpc-engine render_product
```

If that filter misses tests unexpectedly, run:

```bash
cargo test -p lpc-engine
```

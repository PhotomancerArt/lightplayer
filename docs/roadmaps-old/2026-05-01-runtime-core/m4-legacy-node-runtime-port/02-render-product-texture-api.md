# Phase 2: Evolve Render Product Texture/Sampling API

## Scope of Phase

Define the render-product API shape needed for shader/pattern outputs and
fixture sampling. This phase should establish a forward-looking texture-backed
render product without porting shader or fixture nodes yet.

In scope:

- Review and evolve `lp-core/lpc-engine/src/render_product/`.
- Add a texture-backed render product type if needed.
- Keep shader texture memory under render-product / graphics ownership, not
  `RuntimeBufferStore`.
- Provide sampling and optional raw-byte materialization APIs needed by later
  phases.
- Add focused tests around product insertion, sampling, metadata, and raw-byte
  access if implemented.

Out of scope:

- Porting shader, fixture, or output nodes.
- Server wiring.
- Full lazy render-product graph design.
- GPU backend implementation.
- Runtime buffer sync.

## Code Organization Reminders

- Prefer granular files, one concept per file.
- Public abstractions and tests first; helpers near the bottom.
- Keep current render product tests passing.
- If a tactical shortcut is necessary, record it in
  `docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/future.md`.

## Sub-agent Reminders

- Do not commit.
- Stay strictly within phase scope.
- Do not suppress warnings or weaken tests.
- If the existing render-product API cannot support this without a larger design
  choice, stop and report.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `00-notes.md` and `00-design.md` in this plan directory.
- `lp-core/lpc-engine/src/render_product/`.
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`.
- `lp-core/lpc-engine/src/gfx/lp_gfx.rs`.
- `lp-core/lpc-engine/src/legacy/nodes/shader/runtime.rs`.
- `lp-core/lpc-engine/src/legacy/nodes/fixture/runtime.rs`.

Design intent:

- Shader/pattern output is `RuntimeProduct::Render(...)`.
- A render product is opaque and samplable.
- `LpsTextureBuf` must not leak through the public core node/product API.
- Raw bytes may be materialized as an operation for compatibility/sync/testing.

Prefer a small API that can support current CPU/JIT texture backing:

```rust
pub trait RenderProduct {
    fn sample(&self, points: &RenderSampleBatch<'_>) -> Result<RenderSampleResult, RenderProductError>;
}
```

If the existing API already covers point sampling, add only the minimum metadata
and texture product type needed by later phases. A candidate texture product:

```rust
pub struct TextureRenderProduct {
    width: u32,
    height: u32,
    // private storage; no public LpsTextureBuf exposure
}
```

Add methods such as:

- `width()`
- `height()`
- `try_raw_bytes()` or a similarly named optional materialization method if
  easy to support now.

If raw-byte materialization requires a larger storage abstraction, record it in
`future.md` and keep the phase focused on sampling/metadata.

Tests:

- A texture-backed product can be inserted into `RenderProductStore`.
- Sampling returns expected colors for a small deterministic texture/product.
- Render metadata access works.
- Raw-byte materialization works if implemented; otherwise note the deferred item
  in `future.md`.

## Validate

Run:

```bash
cargo test -p lpc-engine render_product
```

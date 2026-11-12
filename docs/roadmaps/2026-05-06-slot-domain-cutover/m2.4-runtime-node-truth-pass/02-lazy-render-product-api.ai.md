# Phase 2: Lazy Render Product API

## Scope Of Phase

Introduce the narrow render-product API needed for lazy full-texture shader execution.

In scope:

- Add a full-texture render request type.
- Extend `RenderProduct` with a mutable render-to-texture capability.
- Add store/context plumbing so nodes can request a full texture render by `RenderProductId`.
- Add tests for default non-renderable behavior and store/context forwarding.

Out of scope:

- Converting `ShaderNode` to the new product.
- Changing fixture flow.
- Adding partial rendering.
- Adding first-class texture resources.
- Rebuilding wire resource sync.

## Code Organization Reminders

- Add one concept per file where useful.
- Suggested new file: `lp-core/lpc-engine/src/render_product/render_texture_request.rs`.
- Keep render-product store helpers in `render_product_store.rs`.
- Keep tests at the bottom of the file they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/render_product/mod.rs`
- `lp-core/lpc-engine/src/render_product/render_product_store.rs`
- `lp-core/lpc-engine/src/render_product/texture_product.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/resolver/resolve_host.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`

Expected API:

```rust
pub struct RenderTextureRequest {
    pub width: u32,
    pub height: u32,
    pub format: lps_shared::TextureStorageFormat,
    pub time_seconds: f32,
}
```

Extend `RenderProduct` with a default method similar to:

```rust
fn render_texture(
    &mut self,
    request: &RenderTextureRequest,
    graphics: Option<&dyn LpGraphics>,
) -> Result<TextureRenderProduct, RenderProductError> {
    let _ = (request, graphics);
    Err(RenderProductError::NotRenderable)
}
```

Add an error variant to `RenderProductError` for non-renderable products. Name it clearly, e.g. `NotRenderable`.

Add mutable store/context plumbing:

- `RenderProductStore::render_texture(...)`.
- `ResolveHost::render_texture(...)` or similarly named method.
- `TickContext::render_texture(...)`.
- `EngineResolveHost` implementation forwards to `RenderProductStore`.

Constraints:

- Keep the method full-texture only.
- Do not make sample-only products allocate or render.
- Do not add texture resources.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine render_product::
cargo test -p lpc-engine node::contexts
```

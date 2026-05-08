# Phase 3: Shader Render Product

## Scope Of Phase

Move shader execution behind a lazy render product and make `ShaderNode` produce that product on slot `output`.

In scope:

- Add `ShaderRenderProduct`.
- Move shader compile/render ownership out of `ShaderNode::tick`.
- Make `ShaderNode::init_resources` allocate/register the lazy shader product.
- Make `ShaderNode` produce `RuntimeProduct::Render` on slot `output`.
- Remove `texture_node_id` and texture-dimension resolution from `ShaderNode`.
- Rename/remove `shader_texture_output_path`; canonical shader slot is `output`.

Out of scope:

- Fixture conversion.
- Loader conversion.
- Texture-node removal.
- Partial rendering.

## Code Organization Reminders

- Suggested new file: `lp-core/lpc-engine/src/render_product/shader_render_product.rs`.
- Keep shader-product internals out of `shader_node.rs` unless they are node-specific.
- Keep small helper functions near their owning type.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/render_product/shader_render_product.rs`
- `lp-core/lpc-engine/src/render_product/mod.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/gfx/*`
- `lp-core/lpc-engine/src/engine/engine.rs`

`ShaderRenderProduct` should:

- own shader source string
- own q32/compile options derived from `ShaderDef`
- compile on first `render_texture` call
- cache `Box<dyn LpShader>`
- allocate a temporary `LpsTextureBuf` matching request dimensions
- call `shader.render(buf, request.time_seconds)`
- return a `TextureRenderProduct` built from the rendered buffer bytes
- return useful `RenderProductError`/messages on missing graphics, compile failure, missing `render()`, allocation failure, or render failure

`ShaderNode` should:

- no longer take `texture_node_id`, placeholder texture width, or placeholder texture height
- allocate the shader render product in `init_resources`
- update produced slot frame/revision during tick
- produce slot `output`, not `texture`
- keep `primary_render_product_id()` only if existing tests/debug helpers need it

Tests to add/update:

- shader node produces slot `output`.
- shader product can render a texture through the new render API using existing graphics test setup.
- old `texture` slot expectations are removed.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine nodes::shader
cargo test -p lpc-engine render_product::
```

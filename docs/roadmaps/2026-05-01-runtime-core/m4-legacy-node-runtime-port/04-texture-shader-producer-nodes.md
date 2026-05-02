# Phase 4: Port Texture And Shader Producer Nodes

## Scope of Phase

Port the texture metadata node and shader/pattern producer path into first-class
core nodes. Shader output should be represented as a render product, not a
runtime buffer.

In scope:

- Add/complete `nodes::core::TextureNode`.
- Add/complete `nodes::core::ShaderNode`.
- Compile GLSL through existing `LpGraphics` paths.
- Produce `RuntimeProduct::Render(...)` for shader/pattern output.
- Preserve render-order semantics where multiple shaders target a texture, or
  record a temporary limitation in `future.md` if this phase cannot complete it.
- Add focused unit/integration tests for compile/produce behavior.

Out of scope:

- Fixture sampling.
- Output flushing.
- Server wiring.
- Full Pattern node rename if it creates unnecessary churn.

## Code Organization Reminders

- Keep texture and shader nodes in separate files.
- Keep shader compile/render helpers below public node impls.
- Avoid exposing `LpsTextureBuf` through the public core node API.
- Record any hacky shortcut in `future.md`.

## Sub-agent Reminders

- Do not commit.
- Stay within phase scope.
- Do not suppress warnings or weaken tests.
- Do not feature-gate embedded shader compilation behind `std`.
- If graphics/render-product ownership needs a design decision, stop and report.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `00-design.md` Q8/render-product decision.
- `lp-core/lpc-engine/src/legacy/nodes/shader/runtime.rs`.
- `lp-core/lpc-engine/src/legacy/nodes/texture/runtime.rs`.
- `lp-core/lpc-engine/src/gfx/`.
- `lp-core/lpc-engine/src/node/node.rs`.
- `lp-core/lpc-engine/src/node/contexts.rs`.
- `lp-core/lpc-engine/src/render_product/`.
- `lp-core/lpc-engine/src/resolver/production.rs`.

Implementation direction:

- Reuse shader compile/execute internals where sensible, but do not wrap
  `LegacyNodeRuntime`.
- Texture metadata should feed render product sizing/format.
- Shader node should produce a render product handle that fixtures can later
  sample.
- Keep output as `RuntimeProduct::Render`.

Tests:

- Shader node can compile a basic GLSL fixture through the existing graphics
  implementation used in tests.
- Shader output appears as `RuntimeProduct::Render`.
- Texture metadata is preserved.
- If render order is implemented, two shaders targeting the same texture apply
  in deterministic order.

## Validate

Run:

```bash
cargo test -p lpc-engine shader
cargo test -p lpc-engine render_product
```

# Phase 2: Engine Visual Sampling API

## Scope Of Phase

Expose direct visual-product sampling through `lpc-engine`.

In scope:
- Add visual sample request/target types suitable for direct shader sampling.
- Add `LpShader` sample method and backend wrappers.
- Add `RenderNode::sample_visual_into`.
- Add `ControlRenderContext::sample_visual_into` and engine dispatch.
- Add shader-node support.

Out of scope:
- Fixture direct sampling strategy.
- Wire/UI debug probes.

## Code Organization Reminders

- Keep product request/result types under `products/visual`.
- Keep shader backend wrappers thin.
- Prefer caller-owned buffers.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:
- `lp-core/lpc-engine/src/products/visual/*`
- `lp-core/lpc-engine/src/gfx/lp_shader.rs`
- `lp-core/lpc-engine/src/gfx/host.rs`
- `lp-core/lpc-engine/src/gfx/native_jit.rs`
- `lp-core/lpc-engine/src/gfx/wasm_guest.rs`
- `lp-core/lpc-engine/src/node/render_node.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`

Expected changes:
- Replace or extend current integer-texel `VisualSampleBatch` with a direct sample request using Q16.16 normalized points.
- Add `VisualSampleTarget<'a>` for caller-owned RGBA16 output slices.
- Add `LpShader::sample_rgba16`.
- `ShaderNode` validates product ownership and calls the shader.
- Engine dispatch mirrors `render_texture_into`.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine shader_node -- --nocapture
cargo test -p lpc-engine output_ -- --nocapture
```


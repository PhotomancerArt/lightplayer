# Phase 3: Render Uniform Resolution

## Scope Of Phase

Move visual shader uniform construction into `ShaderNode` so consumed values are
resolved through the dataflow graph.

In scope:

- Change `LpShader::render` / `sample_rgba16` to receive prepared uniforms
  instead of raw `time`.
- Build visual shader uniforms in `ShaderNode`.
- Resolve visual shader consumed value slots at render/sample time.
- Preserve `outputSize` as a request built-in.

Out of scope:

- Texture uniforms.
- Consumed maps for visual shaders.
- Persisted mutation/writeback.

## Code Organization Reminders

- Keep conversion from `LpValue` to shader ABI values close to existing resolver
  conversion helpers.
- Keep backend-specific files focused on execution, not domain resolution.
- If a small `VisualShaderInputs` helper type makes the code clearer, place it
  in `lp-core/lpc-engine/src/nodes/shader/` or `gfx/uniforms.rs` depending on
  ownership.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/gfx/uniforms.rs`
- `lp-core/lpc-engine/src/gfx/lp_shader.rs`
- `lp-core/lpc-engine/src/gfx/host.rs`
- `lp-core/lpc-engine/src/gfx/native_jit.rs`
- `lp-core/lpc-engine/src/gfx/wasm_guest.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/resolver.rs`

Expected changes:

- Replace backend `build_uniforms(width, height, time)` calls with prepared
  uniforms supplied by `ShaderNode`.
- Build uniforms with:
  - `outputSize = vec2(width, height)` for texture render;
  - `outputSize = vec2(1, sample_count)` for direct sampling;
  - one field per consumed visual shader slot.
- Resolve consumed slot values through the render/materialization context.
  If current `RenderContext` cannot resolve slots, extend it carefully or pass
  enough engine services through the existing session machinery.
- For an unbound consumed slot, use `ShaderSlotDef::default_value()`.
- Convert only value slots via existing `model_value_to_lps_value_f32`.
- Produce useful errors for unsupported consumed map slots.

Tests to add/update:

- A visual shader with consumed `time` sees clock/bus time in render.
- A visual shader with an unbound consumed value uses its default.
- Direct sampling and texture rendering use the same resolved input value.

## Validate

```bash
cargo fmt
cargo test -p lpc-engine shader_node
cargo test -p lpc-engine project_loader
```


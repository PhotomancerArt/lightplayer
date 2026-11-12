## What was built

- Renamed authored render shaders to `kind = "shader/visual"`.
- Replaced visual shader `param_defs` with authored `consumed` slot definitions.
- Registered authored visual shader source bindings for consumed slots.
- Added default `bus#time.seconds` fallback binding for compatible `consumed.time` slots.
- Moved visual shader `time` from an implicit render-request uniform to a normal resolved slot value.
- Updated examples, templates, project builders, tests, and the slot mockup vocabulary.

## Decisions for future reference

#### Visual Shader Kind

- **Decision:** Regular render shaders use `shader/visual`.
- **Why:** The kind now names the product family it produces and leaves room for `shader/compute`.
- **Rejected alternatives:** Keep `shader`; use `visual_shader`.

#### Consumed Slots

- **Decision:** Visual shader dynamic inputs are declared under `consumed`.
- **Why:** The runtime terminology is consumed/produced slots, and this aligns visual shaders with compute shaders.
- **Rejected alternatives:** Keep `param_defs`; use `inputs`.

#### Default Time

- **Decision:** A visual shader with `consumed.time` as `f32` gets a fallback source from `bus#time.seconds` unless it authors a source binding.
- **Why:** Most visual shaders want time, but authored bindings must still win.
- **Rejected alternatives:** Keep `time` as an implicit render-request field; require every shader to author a time binding.

#### Render Uniforms

- **Decision:** `ShaderNode` resolves consumed values during tick and caches prepared uniforms for later product materialization.
- **Why:** Render calls should not re-enter the resolver, and this matches the product model where nodes gather consumed data before lazy execution.
- **Rejected alternatives:** Give `RenderContext` full resolver access; keep backend-specific implicit uniforms.

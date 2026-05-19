# Visual Shader Slots Design

## Scope

Bring regular visual shaders into the same slot/binding vocabulary as compute
shaders so values like `time` are resolved through the dataflow graph instead of
hardcoded engine uniforms.

In scope:

- Rename regular visual shader artifacts from `kind = "shader"` to
  `kind = "shader/visual"`.
- Replace visual shader `param_defs` with `consumed`.
- Register source bindings for visual shader consumed slots.
- Add narrow default binding for compatible `consumed.time`:

  ```text
  bus#time.seconds -> <shader>.time
  ```

- Build visual shader uniforms in `ShaderNode`.
- Keep `outputSize` as render-request built-in data.
- Update examples/templates/tests.

Out of scope:

- Texture inputs.
- Consumed maps for visual shaders.
- Arbitrary visual shader produced slots.
- Artifact writeback.
- General semantic default-binding rules beyond the `time` convention.

## File Structure

```text
lp-core/lpc-model/src/nodes/shader/
  shader_def.rs
  compute_shader_def.rs
  shader_slot_def.rs

lp-core/lpc-engine/src/engine/
  project_loader.rs

lp-core/lpc-engine/src/nodes/shader/
  shader_node.rs

lp-core/lpc-engine/src/gfx/
  uniforms.rs
  lp_shader.rs
  host.rs
  native_jit.rs
  wasm_guest.rs

examples/
  basic/
  basic2/
  fast/
  rocaille/
  perf/baseline/
  perf/fastmath/
```

## Architecture Summary

Visual shaders produce a `VisualProduct` through their runtime state, just as
they do today. Their shader-authored inputs become consumed slots on the node
definition:

```toml
kind = "shader/visual"
glsl_path = "shader.glsl"

[consumed.time]
kind = "value"
value = "f32"

[bindings.output]
target = "bus#visual.out"
```

At load time, the project loader:

- registers authored source bindings for visual consumed slots;
- registers target bindings for `output`;
- if `consumed.time` is a scalar `f32` and no authored binding exists, registers
  a fallback default binding from `bus#time.seconds`.

At render/sample time, `ShaderNode`:

- resolves its consumed value slots through the resolver;
- falls back to slot defaults when unbound;
- converts `LpValue` into `LpsValueF32`;
- builds one uniform struct containing request built-ins plus consumed values;
- passes that uniform value into the graphics backend.

The graphics backend remains responsible for running compiled shaders, but no
longer decides which domain values become uniforms.

## Uniform Boundary

`outputSize` is not modeled as a project binding because it belongs to the
materialization request. The same `VisualProduct` may be rendered at different
sizes by a fixture, debug probe, or future output path.

`time` is modeled as ordinary project data because it is produced by a clock and
should respond to pause, rate, and scrub controls.

## Naming

Use `shader/visual` for regular render shaders. It matches `VisualProduct` and
is less implementation-specific than `shader/pixel` or `shader/render`.

`kind = "shader"` may be accepted as a temporary parser alias if it saves test
churn, but new examples/templates should emit `shader/visual`.


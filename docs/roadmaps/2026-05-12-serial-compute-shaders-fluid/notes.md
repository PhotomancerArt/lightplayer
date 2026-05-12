# Notes

## Scope

Build serial compute shaders as a first-class LightPlayer shader kind, then use
them to generate emitter data for a fluid node:

```text
compute shader -> fluid node -> fixture -> output
```

This is not primarily a fluid feature. Fluid is the proving use case for a
broader domain capability: typed, serial GLSL programs that produce non-visual
data slots.

## User Direction

- Compute shaders are core to the product vision and worth building now.
- TOML is the source of truth for shader slot shapes.
- The UI can make TOML-authored shapes friendly by generating shader headers
  into a bounded region such as:

  ```glsl
  // gen:header
  // generated slot structs/globals here
  // gen:header:end
  ```

- Compute shaders are serial for now. They run once per frame/tick, not as GPU
  workgroups over a dispatch grid.
- Slots map cleanly to shader globals:
  - consumed slots become shader inputs;
  - produced slots become shader outputs;
  - the runtime copies values in before execution and copies values out after.
- Fluid should consume a list or bounded set of emitters from a compute shader.

## Current Codebase State

### Shader Defs And Params

`lpc-model/src/nodes/shader/shader_def.rs` already contains:

- `glsl_path`
- `render_order`
- `bindings`
- `glsl_opts`
- `param_defs: MapSlot<String, ShaderParamDef>`

`ShaderParamDef` is still mostly pressure-harness shaped: label,
description, value type, default, hints. It is close in spirit to what compute
shader slot defs need, but it is not yet a general shader slot definition.

### Shader Runtime

`lpc-engine/src/nodes/shader/shader_node.rs` currently:

- owns GLSL source and compile options;
- compiles lazily through `LpGraphics`;
- exposes `ShaderState.output: VisualProductSlot`;
- implements `RenderNode` for:
  - full texture render;
  - direct RGBA16 point sampling.

This is a visual shader node. Compute shaders should probably share compile
plumbing where practical, but they are semantically different runtime nodes.

### Products And Dataflow

The current product model supports:

- `VisualProduct`
- `ControlProduct`

Both are `LpValue::Product(ProductRef::...)` leaves and flow through slots and
bindings.

For first serial compute work, compute outputs probably do not need a product
handle. A compute shader can tick once per frame and publish produced slot
values directly. A later `ComputeProduct` may be useful for lazy or expensive
compute products, but it is not needed to prove emitter generation.

### Fluid Spike

The old fluid investigation already exists under firmware tests:

- `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`
  - RGB Q32 MSAFluid solver.
- `lp-fw/fw-esp32/src/tests/fluid_demo/`
  - emitters and pulser;
  - ring geometry;
  - sampler;
  - readout;
  - runner.

The solver perf investigation showed fluid is feasible only at modest
resolutions/iteration counts on ESP32-C6. Useful data:

- `N=16, iters=10`: about 2.4M cycles.
- `N=32, iters=4`: about 5.0M cycles, near the 30fps budget for solver alone.
- `N=32, iters=10`: about 9.2M cycles, too heavy for 30fps.

The product path should default to small grids like `20x20`, low iterations,
and direct fixture sampling.

## Architecture Notes

### Serial Compute Shader

The first compute shader is a single serial program:

```glsl
struct Emitter {
    vec2 pos;
    vec2 dir;
    vec3 color;
    float radius;
    float force;
};

in float time;
out int emitter_count;
out Emitter emitters[4];

void main() {
    ...
}
```

Exact syntax is not decided, but TOML owns the shape. The generated header can
declare whichever syntax the compiler supports.

### Shader Slot Definitions

Compute shader artifacts need authored definitions for consumed and produced
slots. These are similar to existing shader params, but more general:

- direction: input/output;
- slot name;
- value shape;
- default for inputs when unbound;
- metadata for UI;
- shader ABI shape.

The UI can edit the TOML slot definitions and regenerate the GLSL header.

### Fluid Emitter Value

Fluid emitters should be a semantic value root in `lpc-model`, likely bounded
for runtime:

```rust
FluidEmitterSet {
    count: u32,
    emitters: [FluidEmitter; MAX]
}

FluidEmitter {
    pos: ...,
    dir: ...,
    color: ...,
    radius: ...,
    force: ...,
}
```

On wire/debug, this can serialize as an `LpValue` structure or list-like value.
In runtime/shader ABI, fixed capacity is preferable.

### Fluid Node

The fluid node should:

- consume `emitters`;
- advance its solver in `tick`;
- produce `VisualProduct`;
- implement `RenderNode` so fixtures/debug probes can sample/render the current
  solver state.

Tick mutates simulation state. Render/sample reads the current simulation.
Multiple consumers/probes must not accidentally advance the fluid.

## Suggested Answers To Open Questions

### Does compute output need `ComputeProduct` now?

Suggested answer: no. Start with produced slot values owned by the compute
shader node state. Introduce `ComputeProduct` only when compute outputs become
lazy, expensive, or too large to copy each frame.

### Is TOML or GLSL the source of truth for shapes?

Answer from user: TOML is source of truth. The shader source can contain a
generated header region for ergonomics.

### Should fluid have hardcoded emitters first?

Suggested answer: no for the main architecture. Hardcoded/demo emitters are
useful for tests, but the roadmap should pressure the real graph by having a
compute shader produce emitter data.

### Should this be a full GPU-style compute shader?

Suggested answer: no. Start serial. GPU/workgroup compute is future work and
belongs to the later wgpu abstraction.

### Is this one milestone or a full roadmap?

Suggested answer: roadmap. It touches model shapes, shader ABI, engine nodes,
fluid runtime, examples, and debug/wire display.


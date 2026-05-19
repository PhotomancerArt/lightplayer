# Serial Compute Shaders And Fluid Emitters

## Motivation

LightPlayer needs shaders that are not visual renderers. Many important effects
are better described as typed data programs: emitter generators, control
generators, audio/touch processors, routing logic, and future GPU-backed
compute stages.

The fluid simulator is the right proving case because it needs external
emitter data:

```text
compute shader -> fluid node -> fixture -> output
```

This tests whether the domain layer can represent heterogeneous dataflow before
the UI/UX is built too deeply around the visual-only path.

## Architecture

### Shader Families

The roadmap introduces a new serial compute shader family alongside visual
shaders:

```text
shader/visual   -> VisualProduct
shader/compute  -> typed produced slots
```

Visual shaders render/sample visual products. Serial compute shaders execute
once per frame and publish typed data slots.

### TOML-Owned Shapes

TOML is the source of truth for shader slot shapes. A compute shader artifact
declares its consumed and produced slots in TOML, including value shapes and UI
metadata.

The UI can keep shader source ergonomic by regenerating a bounded shader header
region:

```glsl
// gen:header
// generated structs and global slot declarations
// gen:header:end
```

This keeps the model editable, inspectable, and codegen-friendly without making
the GLSL parser the source of truth.

### Serial Compute Execution

The first compute shader is serial:

```text
resolve input slots -> write shader globals -> call main once -> read output globals -> publish output slots
```

No workgroups, dispatch grids, barriers, atomics, or storage-buffer semantics
are in scope. This is GLSL as a typed per-frame data program.

### Fluid Emitter Data

Fluid emitters become a semantic value root in `lpc-model`. The initial runtime
shape should be fixed-capacity or otherwise bounded:

```rust
FluidEmitterSet {
    count: u32,
    emitters: [FluidEmitter; MAX]
}
```

This can serialize through `LpValue` for wire/debug, but runtime and shader ABI
should stay allocation-conscious.

### Fluid Node

The fluid node consumes emitter data and produces a visual product:

```text
FluidEmitterSet -> FluidNode solver state -> VisualProduct
```

The node advances simulation in `tick`. Its `RenderNode` implementation samples
or renders the current solver state without advancing it.

## Example Shape

```text
examples/fluid-basic/
  project.toml
  output.toml
  fixture.toml
  emitters.compute.toml
  emitters.glsl
  fluid.toml
```

Possible TOML shape:

```toml
# emitters.compute.toml
kind = "shader/compute"
glsl_path = "./emitters.glsl"

[inputs.time]
type = "f32"

[outputs.emitters]
type = "fluid.emitters"
max_count = 4

[bindings.output]
target = "bus#fluid.emitters"
```

```toml
# fluid.toml
kind = "fluid"

[bindings.emitters]
source = "bus#fluid.emitters"

[bindings.output]
target = "bus#visual.out"
```

## Alternatives Considered

### Hardcoded Fluid Emitters

Simple and useful for a demo, but architecturally weak. It hides the missing
domain feature by combining simulation and choreography in one node.

### Native Rust Emitter Node

Good intermediate fallback and useful for tests, but it does not pressure the
shader/domain boundary that needs validation.

### LFO/Data Processing Graph

Valuable later, but it likely needs several node types and a composition layer
before it can produce rich emitter structs cleanly.

### Full GPU-Style Compute Shader

Too large for the first slice. Serial compute validates the domain and shader
ABI without workgroup/storage-buffer complexity.

## Risks

- Shader ABI for structs and fixed arrays may uncover frontend/LPIR/backend
  gaps.
- Dynamic shader-defined slot shapes may need more registry/shape plumbing than
  current visual shaders.
- `FluidEmitterSet` must be rich enough for real emitters but bounded enough
  for ESP32 memory.
- Fluid solver perf is tight on ESP32-C6; the first example must choose modest
  defaults.
- Header generation must avoid fighting user-authored GLSL edits.

## Scope Estimate

This is a multi-milestone effort. The smallest meaningful end-to-end proof is:

1. compute shader def and shape model;
2. serial compute ABI;
3. compute shader node publishing typed output;
4. fluid node consuming emitters and producing a visual product;
5. example and profiling.


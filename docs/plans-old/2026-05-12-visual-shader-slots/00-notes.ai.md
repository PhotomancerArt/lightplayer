# Visual Shader Slots Notes

## Scope

Plan the cutover for regular render shaders so time and other shader inputs are
resolved through authored slots/bindings instead of hardcoded engine uniforms.

The likely implementation should:

- rename regular shader artifacts away from bare `kind = "shader"` if we want
  a clearer family naming scheme;
- give visual shaders the same slot vocabulary as compute shaders where useful;
- keep `outputSize` as render-request data, not ordinary bound project state;
- resolve `time` from the clock/bus path so pause/scrub can affect visual
  shaders;
- update examples and project/template generators.

Out of scope unless the plan decides otherwise:

- full UI mutation controls beyond the clock controls already added;
- persisted artifact writeback;
- generalized shader input UI/editors beyond what falls out of slot shapes;
- dynamic shader param shape changes for visual shaders beyond a focused slice.

## Current State

### Compute Shaders

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/compute_shader_def.rs`
- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs`
- `examples/fluid/compute.toml`

`ComputeShaderDef` uses:

```toml
[consumed.time]
kind = "value"
value = "f32"

[produced.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
```

`ComputeShaderNode` iterates `def.consumed_slots`, resolves each consumed slot
through the resolver, converts `LpValue` to `LpsValueF32`, and passes those
inputs into `LpComputeShader::tick`.

This is the model visual shaders should converge toward.

### Visual Shaders

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/gfx/uniforms.rs`
- `lp-core/lpc-engine/src/gfx/{host,native_jit,wasm_guest}.rs`
- `lp-core/lpc-engine/src/gfx/lp_shader.rs`

`ShaderDef` currently has:

```rust
pub bindings: BindingDefs,
pub glsl_opts: GlslOpts,
pub param_defs: MapSlot<String, ShaderSlotDef>,
```

But `param_defs` is not used by `ShaderNode`.

`ShaderNode` currently only resolves `glsl_opts` from its def. Render-time shader
inputs are passed through:

```rust
shader.render(target, request.time_seconds)
shader.sample_rgba16(points, out, request.time_seconds)
```

The graphics backends then call:

```rust
build_uniforms(width, height, time)
```

which hardcodes:

```rust
outputSize = vec2(width, height)
time = f32
```

So visual shader `time` currently bypasses the node/bus/binding system.

### Clock And Time Bus

Relevant files:

- `lp-core/lpc-model/src/nodes/clock/*`
- `lp-core/lpc-engine/src/nodes/clock/clock_node.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`

`ClockNode` produces `seconds` and `delta_seconds` on its runtime state root.
Project loader adds a default fallback binding:

```text
clock.seconds -> bus#time.seconds
```

Compute shaders bind consumed time explicitly:

```toml
[bindings.time]
source = "bus#time.seconds"
```

### Binding Registration

Relevant file:

- `lp-core/lpc-engine/src/engine/project_loader.rs`

For compute shaders:

- loader registers optional source bindings for every `consumed_slots` key;
- loader registers target bindings for every `produced_slots` key.

For visual shaders:

- loader registers target binding for `output`;
- loader does not inspect `param_defs` or any consumed slot map;
- therefore visual shader inputs cannot be resolved dynamically yet.

### Naming

Current authored kinds:

- regular visual shader: `kind = "shader"`
- compute shader: `kind = "shader/compute"`

The user noted we may want to rename now, since the family is becoming
`shader/<mode>`.

Possible regular shader names:

- `shader/visual`
- `shader/render`
- `shader/pixel`

The current runtime type is `ShaderNode`, but conceptually it is the node that
produces a `VisualProduct`.

### Examples

Examples currently using `kind = "shader"` and implicit `time`:

- `examples/basic/shader.toml`
- `examples/basic2/shader.toml`
- `examples/fast/shader.toml`
- `examples/rocaille/shader.toml`
- `examples/perf/baseline/shader.toml`
- `examples/perf/fastmath/shader.toml`
- templates in `lp-cli` and `lpa-server`
- builder in `lpc-shared`

Some old examples still include stale `texture_loc`, which is no longer the
clean visual dataflow model.

## Design Pressure

### Uniforms vs Slots

`time` should be an ordinary consumed slot. `outputSize` is different: it is
chosen by the render request/caller and should probably remain render-request
data. Treating `outputSize` as a normal binding would be strange because the
same visual product may be rendered at different sizes for different consumers
or probes.

Suggested direction:

- resolve user/domain inputs from slots/bindings;
- keep render-request built-ins like `outputSize` as backend-provided built-ins;
- make the boundary explicit in names/docs.

### `param_defs` Name

`param_defs` is old vocabulary. Compute shaders use `consumed` and `produced`.
For visual shaders, the likely clean shape is:

```toml
[consumed.time]
kind = "value"
value = "f32"

[bindings.time]
source = "bus#time.seconds"
```

Suggested direction:

- replace or alias `param_defs` with `consumed`;
- support only consumed value slots for this first visual-shader pass;
- leave produced slots as `output` for now, since visual shaders produce a
  `VisualProduct` through node state rather than arbitrary shader-written data.

### Compatibility

The repo has no external users, and the user has repeatedly said aggressive
renaming/removal is fine while defining the domain.

Suggested direction:

- rename `kind = "shader"` to `kind = "shader/visual"` now;
- keep parser alias for `kind = "shader"` only if it reduces churn in tests, but
  prefer updating examples/templates in the same plan;
- if keeping an alias, document it as temporary and do not emit it.

### Render ABI

`LpShader::render` and `sample_rgba16` currently accept a `time: f32` and build
uniforms inside each backend.

Suggested direction:

- change render/sample methods to accept a small visual shader input struct or
  direct `LpsValueF32` uniforms;
- build uniforms in `ShaderNode`, where slot resolution is available;
- backends receive already-materialized uniforms plus render target/sample info.

This keeps shader input resolution in the node/runtime layer rather than hidden
inside graphics backends.

## Open Questions

1. What should regular shader kind be called?

Suggested answer: `shader/visual`. It matches the domain product name and is
less implementation-specific than `shader/pixel`.

2. Should visual shader authored inputs use `consumed` instead of `param_defs`?

Suggested answer: yes. `param_defs` is old language and does not line up with
the produced/consumed slot model.

3. Should `outputSize` remain an implicit render built-in?

Suggested answer: yes. It is request/caller-owned materialization context, not
project-authored data.

4. Should `time` have a default binding like clock output does?

Answer: yes, for the common `time` case. If a visual shader declares a consumed
slot named `time` with `kind = "value"` and `value = "f32"`, and there is no
authored binding for `time`, the loader should register a fallback binding:

```text
bus#time.seconds -> shader.time
```

Authored bindings still win because they use authored priority. This mirrors
the clock node's default output binding:

```text
clock.seconds -> bus#time.seconds
```

Keep the first implementation narrow and name-based; later slot semantics can
replace the `time`/`f32` convention.

5. How much of general param input should be implemented now?

Suggested answer: implement dynamic consumed value slots for visual shaders using
the existing `ShaderSlotDef` and resolver path. Do not implement maps, produced
shader values, or custom UI mutation beyond existing slot data exposure.

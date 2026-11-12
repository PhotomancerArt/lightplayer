# M2 Notes: Serial Compute Shader ABI

## Scope

Teach the shader stack to execute a serial compute shader once and copy typed
slot values in and out.

In scope:

- Add a compute shader compile/execute path alongside visual pixel shaders.
- Define a minimal serial ABI:
  - write consumed slot values into shader-visible storage;
  - call `main` once;
  - read produced slot values back out.
- Support the first fluid emitter shapes, including fixed-capacity sentinel
  map output.
- Validate that authored TOML slot shapes agree with the lowered shader ABI.
- Add tests proving scalar, vector, struct, and fixed-array values move
  Rust -> shader -> Rust.

Out of scope:

- GPU workgroups, dispatch grids, barriers, atomics, or wgpu compute.
- Multiple dispatch instances.
- Arbitrary shader-side dynamic allocation.
- `ComputeShaderNode` and fluid node integration.
- Receiver-side merge semantics for non-leaf bindings.

## User Notes

- Compute shaders are core to the domain and should be built as real
  infrastructure, not as a fluid-only special case.
- TOML remains the source of truth for shader slot shape.
- The shader source may contain a generated header region for ergonomics:

  ```glsl
  // gen:header
  // generated structs and global slot declarations
  // gen:header:end
  ```

- Serial compute means GLSL as a per-frame typed data program:

  ```text
  resolve input slots -> write shader globals -> call main -> read output globals
  ```

- Maps are natural slot-layer data, but GLSL does not have maps. The first
  bridge is a shader-owned mapping, for example:

  ```toml
  mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
  ```

- The native shape name for emitters is `lp::fluid::Emitter`.
- `LpValue -> LpsValue` naming is a possible future rename, but not required
  for this milestone.

## Current State

### Model

- `lpc-model` has `ComputeShaderDef` with:
  - `kind = "shader/compute"`;
  - `glsl_path`;
  - `bindings`;
  - `glsl_opts`;
  - `consumed_slots: MapSlot<String, ShaderSlotDef>` serialized as
    `[consumed.*]`;
  - `produced_slots: MapSlot<String, ShaderSlotDef>` serialized as
    `[produced.*]`.
- `ShaderSlotDef` supports:
  - `kind = "value"` or `kind = "map"`;
  - builtin value refs such as `f32`, `u32`, `vec2`, `vec3`;
  - native value refs such as `lp::fluid::Emitter`;
  - `mapping = { kind = "sentinel", len, key, empty_key }`.
- `FluidEmitter` exists as a native `SlotValue` and `StaticSlotShape`.
  Its native shape name is `lp::fluid::Emitter`; its `LpType` is a struct with
  fields:
  - `id: u32`
  - `pos: vec2`
  - `dir: vec2`
  - `radius: f32`
  - `color: vec3`
  - `velocity: f32`
  - `intensity: f32`
- `shader_header_gen.rs` can generate an M1 compute header, currently using:
  - `layout(binding = N) uniform <ty> <name>;` for consumed value slots;
  - `out <ty> <name>;` or `out <ty> <name>[len];` for produced slots.

### `lp-shader`

- `LpsEngine<E>` currently exposes visual shader compilation:
  - `compile_px`
  - `compile_px_desc`
- `compile_px_desc` does:
  1. parse GLSL through `lps_frontend::compile`;
  2. lower with texture options;
  3. validate texture interface;
  4. validate `render(vec2)` signature;
  5. synthesize texture render and direct sample functions;
  6. compile LPIR into the backend;
  7. return `LpsPxShader`.
- There is no `CompileComputeDesc`, `LpsComputeShader`, or compute entry API.
- `LpsPxShader` erases backend instances behind `PxShaderBackend`.
  The same pattern can be reused for compute.

### Frontend And LPIR

- `lps-frontend` parses GLSL as a vertex stage.
- If source omits `main`, `ensure_vertex_entry_point` injects an empty
  `void main() {}`.
- Lowering already tracks VMContext-backed global storage:
  - `uniform` globals become `LpsModuleSig.uniforms_type`;
  - private globals become `LpsModuleSig.globals_type`;
  - both use std430 layout.
- Existing lower tests cover:
  - private global read/write;
  - vector global writes;
  - globals with initializers and `__shader_init`;
  - multiple globals metadata.
- Existing lowering supports structs, arrays, and array/global access in many
  paths, which is promising for fixed emitter arrays.
- Important wrinkle: current M1 header output uses GLSL `out` globals. The
  lowering code explicitly supports `AddressSpace::Private` and
  `AddressSpace::Uniform`; it is not yet clear whether vertex-stage `out`
  globals become VMContext-backed data or should be avoided for serial compute.

### VM And Backend

- `LpsModuleSig` already exposes:
  - `uniforms_type`
  - `globals_type`
  - `uniforms_offset`
  - `globals_offset`
  - `snapshot_offset`
  - `vmctx_buffer_size`
- `LpvmInstance` currently supports:
  - semantic calls with `LpsValueF32`;
  - flat Q32 calls;
  - visual hot paths `call_render_texture` and `call_render_samples`;
  - `set_uniform` and `set_uniform_q32`.
- There is no public symmetric API for:
  - writing private globals from host before a call;
  - reading private globals after a call.
- Backend instances already maintain a globals snapshot:
  - `init_globals` runs `__shader_init` when present, then snapshots globals;
  - calls reset globals from the snapshot before executing.
- `LpvmDataQ32` already knows how to convert between `LpsType`, bytes, and
  `LpsValueF32`, including arrays and structs. This is likely the right helper
  for global read/write APIs.

## Open Questions

### Resolved So Far

- **Q1 scope boundary:** yes, M2 should prove the shader/VM ABI first. It can
  add a thin engine-facing wrapper if useful, but no `ComputeShaderNode`.
- **Q4 value type:** use `LpsValueF32` for this milestone. Q32 may matter for
  speed later, but that is a broader value/ABI problem.
- **Q5 sentinel map output:** read fixed sentinel-map outputs as raw
  `LpsValueF32::Array` in M2; map materialization belongs to the next layer.
- **Q6 compute entry:** prefer authored `void tick()`; do not silently accept
  an injected empty entry.
- **Q7 validation depth:** validate the supported TOML slot defs against the
  lowered shader ABI.
- **Shader-side direction syntax:** do not add `LP_IN` / `LP_OUT` macros for
  now. TOML owns consumed/produced direction; GLSL can have one ordinary global
  namespace.
- **Global lifecycle:** do not reset globals on each compute tick. Globals are
  allowed to represent persistent runtime state unless a future explicit reset
  convention says otherwise.

### Serial Compute Is LightPlayer-First

The current design should not contort itself into a GPU compute model. M2 is
best treated as a LightPlayer serial data program that happens to be authored
in GLSL because LightPlayer has an on-device GLSL compiler.

That still leaves a future GPU path open because TOML owns the slot schema.
A future WGSL backend can map the same consumed/produced slot definitions to
WGSL storage/uniform bindings when the workload is genuinely parallel. M2
should optimize for the CPU/JIT serial semantics we actually need now.

This also changes the entry-point naming pressure:

- `void main()` is familiar GLSL, but overloaded with shader-stage meaning.
- `void tick()` is more LightPlayer-native and self-documenting for a serial
  per-frame data program.
- `void init()` can be added later for explicit stateful compute.

Decision: generated headers should avoid raw GLSL `in`/`out` global syntax even
though that reads nicely. Those keywords carry graphics-pipeline meaning for
shader engineers and GLSL frontends. We also will not add `LP_IN` / `LP_OUT`
macros in M2; that does not seem worth the extra language surface.

Instead, TOML owns slot direction, and generated GLSL uses one ordinary global
namespace with comments that preserve the domain role:

```glsl
// consumed: time
layout(binding = 0) uniform float time;

// produced: emitters
FluidEmitter emitters[4];

void tick() {
    // ...
}
```

### WGSL Compute Context

WGSL compute shaders are built around a dispatch entry point and bound
resources:

- A compute entry point is marked with `@compute` and `@workgroup_size`.
- The WebGPU API dispatches one or more workgroups. The shader executes once
  per invocation in the dispatch grid.
- Host-visible bulk input/output data is usually exposed as `var<storage, read>`
  or `var<storage, read_write>` buffers with explicit group/binding metadata.
- Uniform buffers are for smaller read-only inputs.
- Entry-point return values are shader-stage outputs in render-style stages;
  compute work generally writes results into storage buffers instead.

For future WGSL, a LightPlayer compute shader that produces emitters would
probably look conceptually like:

```wgsl
struct FluidEmitter {
  id: u32,
  pos: vec2<f32>,
  dir: vec2<f32>,
  radius: f32,
  color: vec3<f32>,
  velocity: f32,
  intensity: f32,
};

@group(0) @binding(0) var<uniform> time: f32;
@group(0) @binding(1) var<storage, read_write> emitters: array<FluidEmitter, 4>;

@compute @workgroup_size(1)
fn main() {
  emitters[0].id = 1u;
  // ...
}
```

This suggests that any future GPU-backed form should be a separate mapping from
the LightPlayer slot schema, not the thing that dictates the first serial CPU
ABI. On embedded GLSL/JIT, the implementation can initially lower slot
bindings to VMContext uniform/private-global storage.

### Q1. Should M2 stop at `lp-shader`, or also add `lpc-engine::gfx` wrappers?

Context: The milestone says "shader engine API" and "engine-facing shader
services", but explicitly excludes fluid node integration. A compute node is
not needed to prove the ABI.

Answer: implement the core compile/execute API in `lp-shader`, then
add only the thinnest `lpc-engine::gfx` wrapper if needed to keep the future
engine integration obvious. Do not add `ComputeShaderNode` in M2.

### Q2. Should generated produced slots be GLSL `out` globals or private globals?

Context: M1 header generation emits `out FluidEmitter emitters[4];`. The
existing VMContext global system clearly supports private globals, while
frontend support for vertex-stage `out` globals is uncertain and may carry
graphics-pipeline meaning that serial compute does not want.

Updated suggested answer: use slot-direction language at the LightPlayer
schema level, but do not model produced compute data as graphics-stage `out`
variables. For the embedded GLSL ABI, generated produced slots should be
writable private globals, for example:

```glsl
FluidEmitter emitters[4];
```

Consumed slots remain `uniform` globals. For future WGSL, the same schema can
map produced slots to `var<storage, read_write>` resources. The authored model
still calls these slots `produced`; the generated shader ABI should use
whatever storage maps cleanly to each backend.

### Q3. Should compute calls reset globals each dispatch?

Context: Current visual and generic calls reset globals from the init snapshot
before execution. For serial compute, produced globals should start from a
clean deterministic state each dispatch, then `main` writes outputs.

Answer: do **not** reset globals on each compute tick. Compute shader globals
are allowed to behave as runtime state. This keeps the GLSL mental model simple:
TOML tells LightPlayer which globals are consumed/produced slots, and all other
globals are ordinary internal shader state.

The dispatch flow becomes:

```text
compile/instantiate -> run global initializers once -> tick loop:
  write consumed globals/uniforms
  call tick()
  read produced globals
```

If a future compute shader wants a deterministic scratch buffer instead of
persistent state, that should be a declared convention or explicit helper
later, not the default lifecycle.

### Q4. Which host-facing value type should compute use?

Context: `lpc-model::LpValue` is the domain value type. `lps-shared` and
`lpvm` use `LpsValueF32` / `LpsValueQ32` for shader ABI values. M2 should not
force a broad rename or merge.

Answer: keep the shader API in terms of `LpsValueF32` for M2, with
small conversion helpers at the `lpc-engine` boundary later. This keeps the
shader stack independent and avoids pulling `lpc-model` into shader crates.

### Q5. What is the first output shape for sentinel maps?

Context: Slot maps are natural domain data; GLSL fixed arrays are the practical
ABI for now. `mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }`
means shader output is an array and runtime materialization should ignore
entries whose key field equals `empty_key`.

Answer: the M2 shader API should read the raw fixed array as
`LpsValueF32::Array`; conversion from sentinel array to `SlotMap<u32, T>` can
be implemented in `lpc-model`/engine in M3. M2 validation should still check
that the named key field exists and has the requested key type.

### Q6. Should `main` be the required entry point?

Context: User examples use `void main()`. The frontend currently injects an
empty vertex `main` if none exists. For compute, silently injecting `main`
would hide a broken shader.

Updated suggested answer: require an authored `void tick()` for
`shader/compute` and validate it exists, takes no parameters, and returns void.
Do not rely on the frontend's empty injected `main` for compute compilation.
If the GLSL frontend requires `main` for parsing, synthesize a tiny wrapper
that calls `tick()` as a compiler detail.

### Q7. How far should type validation go in M2?

Context: The TOML shape is source of truth. The lowered shader metadata is the
ABI reality. We need enough checks to avoid silently reading the wrong bytes,
but not a full generic conversion language yet.

Answer: validate:

- every consumed value slot has a matching uniform global with equivalent
  `LpsType`;
- every produced value slot has a matching private global with equivalent
  `LpsType`;
- every produced sentinel map has a matching private global fixed array whose
  element type matches the mapped value shape;
- the sentinel key field exists and matches the key type.

Reject unsupported slot kinds/mappings with clear diagnostics.

## Likely Files

Likely new or touched files:

- `lp-shader/lp-shader/src/compile_compute_desc.rs`
- `lp-shader/lp-shader/src/compute_shader.rs`
- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lp-shader/src/lib.rs`
- `lp-shader/lp-shader/src/tests.rs` or a new compute-specific test module
- `lp-shader/lpvm/src/instance.rs`
- `lp-shader/lpvm/src/global_data.rs` or similar
- `lp-shader/lpvm/src/lib.rs`
- backend instance impls:
  - `lp-shader/lpvm-native/src/rt_emu/instance.rs`
  - `lp-shader/lpvm-native/src/rt_jit/instance.rs`
  - `lp-shader/lpvm-emu/src/instance.rs`
  - `lp-shader/lpvm-cranelift/src/lpvm_instance.rs`
  - `lp-shader/lpvm-wasm/src/...` if the trait requires it
- `lp-core/lpc-model/src/nodes/shader/shader_header_gen.rs`
- possibly `lp-core/lpc-engine/src/gfx/*` if a thin compute wrapper is useful

## Validation Candidates

- `cargo fmt --check`
- `cargo test -p lps-frontend`
- `cargo test -p lpvm`
- `cargo test -p lp-shader`
- `cargo test -p lpc-model`
- `cargo check -p lpc-engine`
- `cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

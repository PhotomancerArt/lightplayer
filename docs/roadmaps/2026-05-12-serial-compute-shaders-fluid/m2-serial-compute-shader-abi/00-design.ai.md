# M2 Design: Serial Compute Shader ABI

## Scope

M2 adds a serial compute shader ABI to the shader stack. It proves that
LightPlayer can compile GLSL data programs, write consumed slot values into
shader-visible storage, execute `tick()` once, and read produced slot values
back out.

In scope:

- `lp-shader` compile/execute API for serial compute shaders.
- VM/backend global read/write access needed by compute execution.
- Validation between `ComputeShaderDef`/`ShaderSlotDef` and lowered shader ABI.
- Header generation update for produced compute slots.
- Focused tests for scalars, vectors, structs, fixed arrays, fluid emitters,
  persistent globals, and mismatch diagnostics.

Out of scope:

- `ComputeShaderNode` in `lpc-engine`.
- Fluid node integration.
- Receiver-side merge/materialization of sentinel arrays into `SlotMap`.
- GPU/WGSL backend implementation.
- Workgroups, dispatch grids, barriers, atomics, or shader dynamic allocation.

## File Structure

```text
lp-shader/
  lpvm/src/
    global_data.rs
    instance.rs
    lib.rs
    set_uniform.rs
  lpvm-native/src/
    rt_emu/instance.rs
    rt_jit/instance.rs
  lpvm-emu/src/
    instance.rs
  lpvm-cranelift/src/
    lpvm_instance.rs
  lpvm-wasm/src/
    ...
  lp-shader/src/
    compile_compute_desc.rs
    compute_abi.rs
    compute_shader.rs
    engine.rs
    lib.rs
    tests.rs

lp-core/
  lpc-model/src/nodes/shader/
    shader_header_gen.rs
```

The exact wasm backend file path should be discovered during implementation.
The trait addition in `lpvm::LpvmInstance` means every backend implementation
must compile, even if M2 tests mainly exercise host/native-emulated paths.

## Architecture Summary

Serial compute shaders are LightPlayer CPU/JIT data programs. They are not GPU
compute shaders in miniature. TOML owns the slot contract; GLSL is the program
body.

```text
ComputeShaderDef + SlotShapeRegistry
        │
        ├─ validate / generate header
        ▼
GLSL source with ordinary globals and tick()
        │
        ▼
LpsEngine::compile_compute_desc(...)
        │
        ▼
LpsComputeShader
        │
        ├─ set consumed inputs
        ├─ call tick()
        └─ read produced globals
```

## GLSL ABI

Generated consumed slots use uniforms:

```glsl
// consumed: time
layout(binding = 0) uniform float time;
```

Generated produced slots use ordinary private globals:

```glsl
// produced: emitters
FluidEmitter emitters[4];
```

The shader entry point is:

```glsl
void tick() {
    // program body
}
```

If the GLSL frontend requires `main`, the compute compile path may synthesize a
small wrapper:

```glsl
void main() { tick(); }
```

That wrapper is a compiler detail. M2 validation must require an authored
zero-argument `void tick()`.

## Runtime Lifecycle

Compute shader globals are not reset on each tick.

```text
compile/instantiate -> run global initializers once -> tick loop:
  write consumed uniforms
  call tick()
  read produced globals
```

Plain shader globals may be used as persistent internal state. TOML determines
which globals are slot-visible consumed/produced values. All other globals are
internal shader state.

## Main Components

### `LpvmInstance` Global Access

Add symmetric VMContext global access:

```rust
fn set_global(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error>;
fn get_global(&mut self, path: &str) -> Result<LpsValueF32, Self::Error>;
```

The implementation should reuse the same std430 layout and conversion
machinery already used by `set_uniform`. `LpvmDataQ32` already supports
reading/writing structured byte-backed values, arrays, and structs.

`set_global` may be useful for future consumed-private-global inputs. M2 mainly
needs `get_global` for produced slots.

### `CompileComputeDesc`

New descriptor in `lp-shader` containing:

- GLSL source.
- Compiler config.
- A compute ABI description derived from `ComputeShaderDef` where available.
- Optional texture specs only if needed later; no texture work is required for
  M2.

If keeping `lp-shader` independent from `lpc-model` makes this awkward,
`CompileComputeDesc` should accept a shader-local ABI struct instead of
`ComputeShaderDef` directly. The model-to-shader conversion can live above it.

### `LpsComputeShader`

Compiled serial compute shader with erased backend instance, parallel to
`LpsPxShader`.

Expected API shape:

```rust
pub struct LpsComputeShader { ... }

impl LpsComputeShader {
    pub fn meta(&self) -> &LpsModuleSig;
    pub fn tick(&self, inputs: &[(&str, LpsValueF32)]) -> Result<(), LpsError>;
    pub fn get_output(&self, name: &str) -> Result<LpsValueF32, LpsError>;
}
```

Implementation details:

- Apply consumed inputs before calling `tick`.
- Call the lowered `tick` function by name.
- Do not reset globals before `tick`.
- `get_output` reads a produced private global by name.

If it is cleaner, `tick` can return a collection of configured outputs. Avoid
forcing `alloc`-heavy output collection in hot paths if the caller only needs
one produced value.

### Compute ABI Validation

Compile-time validation checks the authored slot ABI against lowered metadata:

- `tick()` exists, takes no params, and returns void.
- Consumed value slots resolve to matching uniform globals.
- Produced value slots resolve to matching private globals.
- Produced sentinel maps resolve to matching fixed private-global arrays.
- Sentinel element type matches the mapped builtin/native value shape.
- Sentinel key field exists and matches the declared key type.

Unsupported slot kinds or mappings should fail with clear diagnostics.

### Header Generation

Update `shader_header_gen.rs` so produced slots generate private globals
instead of GLSL `out` declarations.

Keep direction comments:

```glsl
// consumed: time
layout(binding = 0) uniform float time;

// produced: emitters
FluidEmitter emitters[4];
```

No `LP_IN` / `LP_OUT` macros in M2.

## Testing Strategy

Primary validation is in shader/VM crates:

- scalar input -> scalar output;
- vector input -> vector output;
- internal global persists across ticks;
- struct output;
- fixed array output;
- fluid emitter sentinel array output;
- missing `tick()` fails;
- wrong `tick()` signature fails;
- TOML/header ABI mismatch fails.

Model validation should confirm generated headers no longer use `out` for
produced compute slots.

## Final Validation

Expected final commands:

```bash
cargo fmt --check
cargo test -p lpvm
cargo test -p lp-shader
cargo test -p lpc-model
cargo check -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If changes touch frontend lowering directly, also run:

```bash
cargo test -p lps-frontend
```

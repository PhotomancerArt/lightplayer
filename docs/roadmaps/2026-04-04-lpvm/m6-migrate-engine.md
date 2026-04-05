# M6: Migrate Engine

## Goal

Port `lp-engine` to be generic over `LpvmModule`, making it backend-agnostic.
Ensure firmware builds and end-to-end tests pass.

## Context for Agents

### How lp-engine uses the shader runtime today

`ShaderRuntime` in `lp-core/lp-engine/src/nodes/shader/runtime.rs`:

- **Fields**: `jit_module: Option<JitModule>`, `direct_call: Option<DirectCall>`
- **Compile**: `lpir_cranelift::jit(source, options)` → `JitModule`, then
  `module.direct_call("main")` → `DirectCall`
- **Render** (per pixel): `direct_call.call_i32_buf(&vmctx, &args, &mut ret_buf)`
  with Q32 args (frag_coord, output_size, time) → Q32 RGBA output
- **Other**: `shed_optional_buffers` clears module and call handle

The engine directly uses `lpir_cranelift` types. It does not use
`GlslExecutable`.

### What changes

`ShaderRuntime` (or the equivalent type) becomes generic over `M: LpvmModule`.
Instead of `JitModule` and `DirectCall`, it holds `M` (the module) and
`M::Instance` (the instance, or a prepared call handle).

The compile step becomes: GLSL → naga → LPIR → `M::compile(ir)` → module.
The render step becomes: `instance.call(...)` or equivalent hot-path method.

### Generic engine architecture

```rust
pub struct ShaderRuntime<M: LpvmModule> {
    module: Option<M>,
    instance: Option<M::Instance>,
    // ... config, texture handle, errors, state
}
```

The concrete backend is selected by the firmware crate:

```rust
// fw-esp32
type ShaderRt = ShaderRuntime<lpvm_cranelift::CraneliftModule>;

// fw-wasm (future)
type ShaderRt = ShaderRuntime<lpvm_wasm::WasmModule>;
```

### The hot path

The render loop calls a shader function per pixel (or per-pixel-batch). This
must be as fast as possible. Today it's a raw function pointer call via
`DirectCall::call_i32_buf`.

The LPVM trait must provide an equivalent zero-overhead path. The engine should
be able to get a "prepared call" from the instance that avoids per-call name
lookup and value marshaling.

If the trait design from M1/M2/M3 has a `call(name, args)` method that
marshals `LpvmValue`, the engine should NOT use that on the hot path. There
must be a way to call with raw i32 args (Q32) without conversion overhead.

Options (should be resolved by M3):
1. Trait has an associated "call handle" type that backends can optimize
2. Engine uses a backend-specific fast-path method in addition to the trait
3. Trait has both `call` (ergonomic) and `call_raw` (fast) methods

### Other engine types that may need generics

- `ProjectRuntime` — owns `ShaderRuntime`s and other node runtimes
- `NodeRuntime` — enum of different node types including shader
- `NodeInitContext`, `RenderContext` — may need to carry backend type

Evaluate how far the generic parameter needs to propagate. If it infects too
many types, consider a trait object at the project level (where the overhead
is acceptable — project-level operations are not per-pixel).

### Firmware integration

`fw-esp32` currently depends on `lp-engine` and `lpir-cranelift`. After
migration:

- `fw-esp32` depends on `lp-engine` (generic) and `lpvm-cranelift` (concrete)
- It monomorphizes the engine with the Cranelift backend
- `lpir-cranelift` dependency may be removed (replaced by `lpvm-cranelift`)

`fw-emu` follows the same pattern.

## End-to-End Tests

The key validation is `fw-tests`:

```bash
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
```

These tests compile real shaders and render frames. They must pass with the
LPVM-based engine.

Also validate firmware builds:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

And host checks:

```bash
cargo check -p lp-server
cargo test -p lp-server --no-run
```

## What NOT To Do

- Do NOT add `#[cfg(feature = "std")]` to any compile/execute path. The
  embedded JIT is the product.
- Do NOT use trait objects (`dyn LpvmModule`) on the per-pixel render path.
  Use generics.
- Do NOT change the Q32 calling convention. The engine calls shaders with Q32
  (fixed-point) values. This is a separate concern from LPVM.
- Do NOT break `fw-tests`. If they fail, the bug is in the migration.

## Done When

- `lp-engine` is generic over `LpvmModule`
- `fw-esp32` builds with `lpvm-cranelift` as the backend
- `fw-emu` builds
- `fw-tests` pass (`scene_render_emu`, `alloc_trace_emu`)
- Embedded check passes: `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`
- Host checks pass: `cargo check -p lp-server`, `cargo test -p lp-server --no-run`
- No performance regression on the render path (measured or at least verified
  by examining generated code)

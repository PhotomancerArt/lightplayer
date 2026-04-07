# M5: Migrate Filetests to LPVM Traits

## Scope of Work

Port `lps-filetests` from the `GlslExecutable` trait to LPVM traits (`LpvmEngine`, `LpvmModule`, `LpvmInstance`). All three backends (Cranelift JIT, WASM, RV32 emulator) run through the unified LPVM interface.

### In Scope

- Replace `GlslExecutable` usage in `lps-filetests` with LPVM traits
- Update test execution to use `LpvmEngine::compile()` → `LpvmModule::instantiate()` → `LpvmInstance::call()`
- Update the filetest runner to be generic over `LpvmEngine` or use a dispatch mechanism for backend selection
- Migrate `LpirJitExecutable` to use `lpvm-cranelift`'s `CraneliftEngine`
- Migrate RV32 executable to use `lpvm-emu`'s `EmuEngine`
- Migrate WASM executables to use `lpvm-wasm`'s engines
- All existing filetests must pass with the new infrastructure
- Remove old `GlslExecutable` implementations from filetest code

### Out of Scope

- New filetests for shared memory features (textures, cross-shader data)
- Removing `GlslExecutable` trait definition from `lps-exec` (M7)
- Performance optimization of the filetest path

## Current State

### LPVM Trait Implementations (Ready)

All three backends now implement `LpvmEngine`:

1. **Cranelift JIT** (`lpvm-cranelift/src/lpvm_engine.rs`)
   - `CraneliftEngine` implements `LpvmEngine<Error=CompilerError, Module=CraneliftModule>`
   - Supports `std` (host heap) and `no_std` (bump arena) memory

2. **RV32 Emulator** (`lpvm-emu/src/engine.rs`)
   - `EmuEngine` implements `LpvmEngine<Error=CompilerError, Module=EmuModule>`
   - Compiles to RV32 object, links with builtins, runs in emulator

3. **WASM** (`lpvm-wasm/src/rt_wasmtime/engine.rs`, `rt_browser/engine.rs`)
   - `WasmLpvmEngine` (wasmtime) implements `LpvmEngine`
   - `BrowserLpvmEngine` (browser/WebAssembly) implements `LpvmEngine`

### Current Filetest Architecture

**`lps-filetests/src/test_run/execution.rs`**
- Uses `dyn GlslExecutable` trait from `lps_exec`
- Dispatches based on `LpsType` return type: `call_f32`, `call_i32`, `call_bool`, `call_vec`, `call_mat`, etc.
- Formats errors with optional emulator state

**Backend-specific executables (OLD API):**

| File | Current Implementation | New Target |
|------|------------------------|------------|
| `lpir_jit_executable.rs` | Wraps `JitModule` → `GlslExecutable` | Use `CraneliftEngine` |
| `lpir_rv32_executable.rs` | Custom RV32 glue | Use `EmuEngine` |
| `wasm_runner.rs` | Custom WASM glue | Use `WasmLpvmEngine` |
| `wasm_link.rs` | WASM builtin linking | May be redundant with new trait |

**Test runner dispatch (`run_detail.rs`):**
- Selects backend based on target string (`jit.q32`, `rv32.q32`, `wasm.q32`)
- Creates appropriate `GlslExecutable` implementation
- Runs through `execute_function` in `execution.rs`

### Key Differences: Old vs New API

**OLD (`GlslExecutable`):**
```rust
trait GlslExecutable {
    fn call_f32(&mut self, name: &str, args: &[LpsValue]) -> Result<f32, GlslError>;
    fn call_i32(&mut self, name: &str, args: &[LpsValue]) -> Result<i32, GlslError>;
    fn call_vec(&mut self, name: &str, args: &[LpsValue], len: usize) -> Result<Vec<f32>, GlslError>;
    // ... per-type methods
}
```

**NEW (`LpvmInstance`):**
```rust
trait LpvmInstance {
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, LpsError>;
}
```

The new API is simpler: one `call` method returns `LpsValue` (enum) instead of per-type methods.

## Questions

### Q1: ✅ ANSWERED: Engine + Module per test file, Instance per test case

**Context:** The LPVM trait design has `LpvmEngine` as the factory that compiles modules. Engines own shared memory arenas (for textures, globals). Modules are cheap; engines carry the heavy resources.

**Current wrong behavior:** Creating executable (engine + compile) per **test case** (line) — causes redundant compilation of the same GLSL file hundreds of times.

**Correct design:**
- **Engine**: One per **test file** (shared arena, linked builtins, heavy setup)
- **Module**: One per **test file** (compile GLSL → IR once, reuse)
- **Instance**: Fresh per **test case** (lightweight per-invocation context)

This is much faster and matches real usage: one engine context, compile once, run many instances.

### Q2: ✅ ANSWERED: Add `debug_state()` to `LpvmInstance` trait

**Context:** Current `execution.rs` uses `executable.format_emulator_state()` for RV32 debugging. This is a **gap in the LPVM API** — we need this capability.

**Answer:** Add optional debug state to `LpvmInstance`:

```rust
trait LpvmInstance {
    // ... existing methods ...

    /// Return debug state for error reporting (registers, PC, stack, etc.)
    /// Default implementation returns `None`.
    fn debug_state(&self) -> Option<String> {
        None
    }
}
```

- `EmuInstance` implements with emulator registers, PC, etc.
- JIT and WASM instances return `None`
- Filetest runner uses this in error formatting

This is a **trait addition in `lpvm`** crate as part of this milestone.

### Q3: ✅ ANSWERED: Add `call_q32()` to `LpvmInstance` for exact Q32 testing

**Context:** `LpsValue::F32(f32)` loses Q32 precision. Filetests testing Q32 builtins need exact `i32` Q16.16 values in/out.

**Problem:** `call()` returns `LpsValue` — Q32 values go through `f32` conversion, which doesn't represent all Q16.16 values exactly.

**Answer:** Add `call_q32()` to `LpvmInstance`:

```rust
trait LpvmInstance {
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, LpsError>;
    
    /// Call with exact Q16.16 i32 values, bypassing f32 conversion.
    /// Default impl converts through LpsValue (may lose precision).
    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, LpsError> {
        // Default: convert args i32 -> Q32 LpsValue variants
        // Call self.call()
        // Convert results back to i32
    }
}
```

**Implementations:**
- `CraneliftModule::Instance` (Q32 mode): exact Q32 calling via generated code
- `EmuInstance` (Q32 mode): exact via emulator raw i32 calling
- WASM instances: default implementation (filetests don't rely on WASM for exact Q32)

**Delete `q32_exec_common`** — its logic moves into the `call_q32()` implementations.

This is a **breaking change in `lpvm`** crate, requiring updates to all `LpvmInstance` impls.

### Q4: How do we handle the `// backend: xxx` directive dispatch?

**Context:** Filetests have backend directives like `// backend: rv32`, `// backend: jit`, `// backend: wasm`. Current runner matches to create executables.

**Answer:** Use **engine-per-file, instance-per-case** pattern:

```rust
struct TestFileContext {
    engine: FiletestEngine,  // created once per file
    module: Box<dyn LpvmModule>, // compiled once per file
}

enum FiletestEngine {
    Cranelift { engine: CraneliftEngine, options: CompileOptions },
    Emu { engine: EmuEngine, options: CompileOptions },
    Wasm { engine: WasmLpvmEngine, options: WasmOptions },
}

impl FiletestEngine {
    fn compile(&self, ir: &IrModule) -> Box<dyn LpvmModule> { ... }
}
```

Runner matches backend string → creates `TestFileContext` → per test case calls `module.instantiate()` → `instance.call()` or `call_q32()`.

### Q5: ✅ ANSWERED: Only migrate `WasmLpvmEngine` (wasmtime)

**Context:** `lpvm-wasm` has two engines: `WasmLpvmEngine` (wasmtime, host) and `BrowserLpvmEngine` (browser, web demo). Filetests run on host.

**Answer:** Only migrate `WasmLpvmEngine` for filetests. `BrowserLpvmEngine` is for `lp-app/web-demo`, not CI. Add default `call_q32()` implementation to `WasmLpvmInstance`.

## Summary of Required Changes

### `lpvm` crate (trait additions)
- `LpvmInstance::debug_state() -> Option<String>` — default `None`
- `LpvmInstance::call_q32(args: &[i32]) -> Result<Vec<i32>, LpsError>` — default via `LpsValue`

### `lpvm-cranelift` crate
- Implement `call_q32()` for Q32 mode: direct Q32 raw calling
- Implement `debug_state()`: return `None` (or stack trace if available)

### `lpvm-emu` crate
- Implement `call_q32()`: raw i32 calling through emulator
- Implement `debug_state()`: emulator registers/PC/state

### `lpvm-wasm` crate
- `WasmLpvmInstance`: default `call_q32()` via `LpsValue`
- `debug_state()`: return `None`

### `lps-filetests` crate
- Delete `q32_exec_common.rs`, `lpir_jit_executable.rs`, `lpir_rv32_executable.rs`
- Delete `wasm_link.rs`, `wasm_runner.rs` (or heavily simplify)
- Create new `engine.rs` with `FiletestEngine` enum
- Update `execution.rs` to use `LpvmInstance::call()` / `call_q32()`
- Update `run_detail.rs` for engine-per-file pattern
- All filetests pass on `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32`

## Notes

- The `lps-exec` crate (with `GlslExecutable`) is in `lp-shader/legacy/`. M5 only removes usage from filetests; M7 removes the trait entirely.
- Current filetest targets: `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32`
- The new engines support both float modes via `CompileOptions` / `WasmOptions`
- The `lpvm-emu` engine requires `std` (for object linking), even though it targets `no_std` RISC-V

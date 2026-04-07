# M5: Migrate Filetests to LPVM Traits

**Plan docs:** [`00-design.md`](./00-design.md) (architecture + file tree). **All phases in one page:** [`PHASES.md`](./PHASES.md). **Per-phase files:** `01`–`08` (`01-phase-lpvm-instance-call-q32.md` … `08-phase-cleanup-validation.md`).

**Prerequisite (done):** [`lps-value-q32-restructure`](../2026-04-07-lps-value-q32-restructure/00-design.md) — `LpsValueQ32`, `lpvm_abi`.

## Scope of Work

Port `lps-filetests` from the `GlslExecutable` trait to LPVM traits (`LpvmEngine`, `LpvmModule`, `LpvmInstance`). All three backends (Cranelift JIT, WASM, RV32 emulator) run through the unified LPVM interface.

### In Scope

- Replace `GlslExecutable` usage in `lps-filetests` with LPVM traits
- Update test execution to use `LpvmEngine::compile()` → `LpvmModule::instantiate()` → `LpvmInstance::call()` and, for Q32 targets, **`LpvmInstance::call_q32()`** when tests need the exact ABI path without `LpsValueF32` float lanes
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

## Update: Q32 bridge already landed (commit `294620fc`)

Before M5 implementation, **`LpsValueQ32` + `lpvm::lpvm_abi`** replaced the old F64 path and the “everything is `f32` in the call ABI” problem for Q32 mode.

- **Design:** [`00-design.md` in sibling plan](../2026-04-07-lps-value-q32-restructure/00-design.md) (phases `01`–`08` in that directory).
- **`lps-shared`:** `LpsValueQ32` holds `Q32` (from `lps-q32`) for float lanes; conversions to/from `LpsValueF32` via `lps_value_f32_to_q32` / `q32_to_lps_value_f32` (saturating F32→Q32 for user-facing values).
- **`lpvm`:** `lpvm_abi.rs` — `flatten_q32_arg`, `decode_q32_return`, `GlslReturn`, `CallError`; ABI layer is **`Vec<i32>`** (`Q32::to_fixed` / `from_fixed` for float components).
- **`lpvm-cranelift` / `lpvm-emu`:** wired to the Q32 path; **`LpvmInstance::call` remains `LpsValueF32` in/out** — backends convert to `LpsValueQ32` + flatten internally.
- **`lps-filetests`:** `q32_exec_common.rs` was **shrunk** (still bridges `GlslExecutable` → `Q32ShaderExecutable` with `LpsValueQ32`); **`lpir_jit_executable` / `lpir_rv32_executable`** updated to the new types.

**M5 still adds `LpvmInstance::call_q32`:** The `LpsValueQ32` + `lpvm_abi` stack is the **implementation substrate**; we still want an explicit **`call_q32`** on the trait so **filetests (and other hosts)** can invoke Q32 shaders **without marshaling float components through `f32`**. Default implementation can delegate via `LpsValueF32` (lossy); **JIT + emulator** implement the exact path by reusing the same machine entry as today’s internal Q32 call. **WASM** can keep the default until filetests need otherwise.

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

**NEW (`LpvmInstance`) — today vs after M5:**
```rust
trait LpvmInstance {
    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error>;

    /// Q32 mode: flat `i32` words in ABI order (same concatenation as `flatten_q32_arg` per param).
    /// Return value is flattened return words (void → empty `Vec`).
    /// Default: synthesize via F32 round-trip (lossy); override for exact path on JIT/emu.
    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> { ... }

    fn debug_state(&self) -> Option<String> { None }
}
```

**Target:** `call` for normal host / float-oriented use; **`call_q32` for filetests and any caller that wants raw Q16.16 lanes** without `LpsValueF32` float components. Internally, `call_q32` should share code with the existing **`LpsValueQ32` + `lpvm_abi`** path (flatten/decode), not duplicate semantics ad hoc.

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

### Q3: ✅ ANSWERED: `LpsValueQ32` + `lpvm_abi` **and** `LpvmInstance::call_q32`

**Foundation (commit `294620fc`):** Three-layer model — `LpsValueF32` → `LpsValueQ32` → flat `Vec<i32>` via `lpvm_abi`; JIT/emu already call the machine with flattened words and decode to `LpsValueQ32`.

**M5 addition:** Expose **`call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error>`** on **`LpvmInstance`** (exact signature can gain `GlslReturn` / metadata if needed, but **flat `i32` ABI** is the intent). Filetests on **Q32 targets** should prefer **`call_q32`** so expectations can be driven from **raw fixed-point words** or from **`LpsValueQ32`** built without going through `f32` for float lanes.

**Implementation:** Reuse **`flatten_q32_arg` / `decode_q32_return`** and existing backend entry points — **no second semantic**. Default body on trait may convert args/results through `LpsValueF32` for backends that only need the slow path.

**`q32_exec_common`:** Can thin out to “build flat args from parsed expectations + call `instance.call_q32` + compare decoded `LpsValueQ32`” once `GlslExecutable` is gone.

### Q4: ✅ ANSWERED: `// backend` + engine/module per file

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

Runner matches backend string → creates `TestFileContext` → per test case `instantiate()` → **`call_q32`** on `jit.q32` / `rv32.q32` / `wasm.q32` (and **`call`** for `.f32` targets or when comparing via float tolerance only).

### Q5: ✅ ANSWERED: Only migrate `WasmLpvmEngine` (wasmtime)

**Context:** `lpvm-wasm` has two engines: `WasmLpvmEngine` (wasmtime, host) and `BrowserLpvmEngine` (browser, web demo). Filetests run on host.

**Answer:** Only migrate `WasmLpvmEngine` for filetests. `BrowserLpvmEngine` is for `lp-app/web-demo`, not CI.

## Summary of Required Changes

### `lpvm` crate
- **`LpvmInstance::call_q32(name, args: &[i32]) -> Result<Vec<i32>, Self::Error>`** — add with **default impl** (F32 round-trip); document ABI = concatenated `flatten_q32_arg` word order.
- **`LpvmInstance::debug_state() -> Option<String>`** — add with default `None`.

### `lpvm-cranelift` / `lpvm-emu` / `lpvm-wasm` crates
- **`call_q32`:** **Exact** override on Cranelift + emu (same machine path as today’s internal Q32 call); WASM may use default until needed.
- **`debug_state()`:** rich on **emu**, `None` elsewhere initially.

### `lps-filetests` crate
- **`LpvmEngine` → `LpvmModule` → `LpvmInstance`**: **`call_q32`** for Q32 targets, **`call`** for f32 targets.
- **Engine per file, module per file, instance per test case**.
- Remove redundant `GlslExecutable` wrappers; **`q32_exec_common`** becomes thin glue around **`call_q32`** + `lpvm_abi` decode helpers where useful.
- **`execution.rs`:** `debug_state()` on errors.
- All filetests pass on `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32`

## Suggested phase order (summary)

1. **`lpvm`** — `LpvmInstance::call_q32` + `debug_state` (+ defaults)
2. **`lpvm-cranelift`** — exact `call_q32`, `debug_state`
3. **`lpvm-emu`** — exact `call_q32`, rich `debug_state`
4. **`lpvm-wasm`** — wasmtime + browser: defaults or exact `call_q32` if tests require
5. **`lps-filetests`** — engine + compiled module per file; wire `run_detail` / `compile`
6. **`lps-filetests`** — `execution.rs`: `call` / `call_q32`, error formatting
7. **`lps-filetests`** — remove `GlslExecutable` wrappers; thin `q32_exec_common`
8. **Cleanup** — full matrix, `summary.md`, move plan to `plans-done`, commit

## Notes

- The `lps-exec` crate (with `GlslExecutable`) is in `lp-shader/legacy/`. M5 only removes usage from filetests; M7 removes the trait entirely.
- Current filetest targets: `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32`
- The new engines support both float modes via `CompileOptions` / `WasmOptions`
- The `lpvm-emu` engine requires `std` (for object linking), even though it targets `no_std` RISC-V

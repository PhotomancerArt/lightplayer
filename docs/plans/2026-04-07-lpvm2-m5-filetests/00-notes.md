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

### Q3: Should we keep the `q32_exec_common` module or fold it into the new structure?

**Context:** `q32_exec_common.rs` has shared Q32 marshaling logic used by JIT and RV32 executables:
- `args_to_q32()` — convert `LpsValue` args to Q32 raw `i32`s
- `call_*_from_q32()` — convert Q32 results back to `LpsValue`
- `Q32ShaderExecutable` trait (bridges `GlslExecutable` to Q32 calling)

With `LpvmInstance::call()` returning `LpsValue` directly, some of this may be redundant.

**Options:**
1. Keep `q32_exec_common` but adapt for `LpvmInstance` marshaling
2. Inline the marshaling logic into new executable wrappers
3. Remove entirely — `LpvmInstance` handles marshaling internally

**Suggested:** Option 3 — the new `LpvmInstance::call()` already works with `LpsValue`. The common Q32 conversion logic may not be needed. However, we should verify that the new engines handle Q32 values correctly for filetest expectations.

### Q4: How do we handle the `// backend: xxx` directive dispatch?

**Context:** Filetests have backend directives like `// backend: rv32`, `// backend: jit`, `// backend: wasm`. The current runner matches these to create appropriate executables.

**Current flow:**
1. Parse test file → extract directives
2. For each test case, check if backend is supported
3. Create executable for that backend
4. Run and compare

**Options:**
1. Keep string matching, map to engine factory: `match backend { "rv32" => EmuEngine::new(), ... }`
2. Create a `FiletestEngine` enum wrapping all three: `enum FiletestEngine { Cranelift(CraneliftEngine), Emu(EmuEngine), Wasm(WasmLpvmEngine) }`
3. Generic runner: `fn run_with_engine<E: LpvmEngine>(engine: &E, test: &TestCase)`

**Suggested:** Option 2 — an enum wrapper keeps the dispatch explicit and avoids generics complexity. Each variant holds the engine + any backend-specific config (compile options, float mode).

### Q5: What about the WASM browser backend? Is it needed for filetests?

**Context:** `lpvm-wasm` has two engines:
- `WasmLpvmEngine` (wasmtime) — for host tests
- `BrowserLpvmEngine` (browser WebAssembly) — for web demo

Filetests run on host, not browser.

**Options:**
1. Only migrate `WasmLpvmEngine` (wasmtime) for filetests
2. Migrate both, even though browser isn't used in tests
3. Keep WASM browser separate entirely

**Suggested:** Option 1 — only migrate `WasmLpvmEngine` for filetests. `BrowserLpvmEngine` is for the web demo (`lp-app/web-demo`), not CI/filetests.

## Notes

- The `lps-exec` crate (with `GlslExecutable`) is in `lp-shader/legacy/`. M5 only removes usage from filetests; M7 removes the trait entirely.
- Current filetest targets: `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32`
- The new engines support both float modes via `CompileOptions` / `WasmOptions`
- The `lpvm-emu` engine requires `std` (for object linking), even though it targets `no_std` RISC-V

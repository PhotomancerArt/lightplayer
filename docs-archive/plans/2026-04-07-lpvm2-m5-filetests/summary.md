# M5: LPVM Filetests - call_q32, debug_state, GlslExecutable Removal

## Summary

Migrated filetests from the legacy `GlslExecutable` abstraction to the new `LpvmEngine`/`LpvmInstance` API with flat Q32 value support.

## What Shipped

### Core API Additions (`lpvm` crate)

- **`LpvmInstance::call_q32`** — Execute a compiled function using flat Q32 argument/return encoding
- **`LpvmInstance::debug_state`** — Optional hook for capturing execution state on trap (fuel, PC, registers)
- **`lpvm_abi` module** — ABI helper functions:
  - `unflatten_q32_args` — Convert flat u32 words to `LpsValueQ32` based on signature
  - `flatten_q32_return` — Convert `LpsValueQ32` return to flat u32 words
  - `flat_q32_words_from_f32_args` — Convert f32 test inputs to Q32 flat encoding

### Backend Implementations

| Backend | `call_q32` | `debug_state` | Notes |
|---------|------------|---------------|-------|
| `lpvm-cranelift` | ✅ | ✅ | Native JIT via Cranelift, debug captures last trap |
| `lpvm-emu` | ✅ | ✅ | RISC-V emulator, `last_debug` populated on ecall/trap |
| `lpvm-wasm` (wasmtime) | ✅ | ❌ (None) | Browser & wasmtime runtimes |
| `lpvm-wasm` (browser) | ✅ | ❌ (None) | |

### Filetests Architecture (`lps-filetests`)

- **Lifecycle change**: One `LpvmEngine` + `LpvmModule` per test file; one `LpvmInstance` per test case
- **New `CompiledShader` type**: Wraps engine/module without the legacy `GlslExecutable` glue
- **Removed**:
  - `lpir_jit_executable.rs`
  - `lpir_rv32_executable.rs`
  - `wasm_runner.rs`
  - `q32_exec_common.rs`
  - `lps-exec` dependency from `Cargo.toml`
- **Simplified `run_detail.rs`**: Removed `GlslExecutable` trait, directly use `LpvmInstance::call_q32`
- **Debug output**: Execution errors include `debug_state` dump (fuel, PC) when available

### wasm_link Note

`wasm_link.rs` remains in the codebase for `tests/lpfn_builtins_memory.rs`. It will be removed in M6 when builtins memory migrates to the new engine.

### Target Matrix Reality Check

Phase documentation listed 6 targets (jit.q32, jit.f32, rv32.q32, rv32.f32, wasm.q32, wasm.f32). The actual `ALL_TARGETS` in `lps-filetests/src/targets/mod.rs` only defines 3 Q32 targets currently:
- `wasm.q32`
- `jit.q32` (default for `cargo test`)
- `rv32.q32`

F32 targets are future work.

## Test Results

Post-implementation filetest run:
- **jit.q32**: Default host JIT - passing (baseline)
- **rv32.q32**: 4357 pass, 52 fail, 633 unimpl, 52 unsupported, 78 compile-fail
- **wasm.q32**: 4311 pass, 34 fail, 694 unimpl, 56 unsupported, 81 compile-fail
- **15 tests newly pass** — M5 changes fixed some previously-failing cases
- **16 test files fail** — pre-existing issues (LPFX builtins, struct support), not M5 regressions

## Dependencies Changed

### Removed
- `lps-exec` from `lps-filetests/Cargo.toml`
- `GlslExecutable` imports and trait implementations

### Added
- `lpvm` dependency where needed for `LpvmInstance` API

## Migration Notes

Code using the old pattern:
```rust
let exe = compile_glsl_to_executable(&shader, target);
let result = exe.run(&args);
```

Should now use:
```rust
let (engine, module) = compile_shader(&shader)?;
let instance = engine.instantiate(&module)?;
let flat_args = flat_q32_words_from_f32_args(&sig, &args);
let flat_ret = instance.call_q32(&func, &flat_args)?;
let result = unflatten_q32_return(&sig, &flat_ret);
```

## Build Fix

Fixed `lpvm-cranelift/build.rs` path resolution for `lps-builtins-emu-app`:
- `find_workspace_root` now correctly returns the workspace root directory
- Path construction uses `join("target")` instead of `join("../../target")`

## Commit

Conventional commit format:
```
feat(filetests): migrate to LpvmEngine and add call_q32

- Add LpvmInstance::call_q32 and debug_state
- Engine/module per filetest file; instance per case
- Remove GlslExecutable from lps-filetests
- Fix build.rs path resolution for builtins embedding
```

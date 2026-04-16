# M7 Cleanup Summary

## Overview

Final cleanup milestone for LPVM2. Deleted obsolete code, removed dead traits
and legacy crates, consolidated duplicate code paths.

## What Was Deleted

### Legacy Crates (from `lp-shader/legacy/`)

| Crate | What It Was | Why Deleted |
|-------|-------------|-------------|
| `lps-exec` | `GlslExecutable` trait | Superseded by `LpvmEngine`/`LpvmModule`/`LpvmInstance` traits |
| `lps-wasm` | Old WASM emitter | Replaced by `lpvm-wasm` with LPVM trait implementation |
| `lps-builtins-wasm` | Old builtins WASM build system | Build now handled by `lps-builtins-gen-app` or integrated |

### Obsolete APIs (from `lpvm-cranelift`)

| API | Replacement |
|-----|-------------|
| `jit()` function | `CraneliftEngine::compile()` |
| `JitModule` struct | `CraneliftModule` |
| `jit_from_ir()` | `CraneliftEngine::compile_from_ir()` (if exists) |
| `jit_from_ir_owned()` | Engine method with owned IR |

### Obsolete Modules

| File | What It Did | Replacement |
|------|-------------|-------------|
| `lpvm-emu/src/emu_run.rs` | Helper functions for emu execution | `EmuInstance::call()`/`call_q32()` |
| `lps-filetests/src/test_run/wasm_link.rs` | wasmtime linking for tests | `lpvm-wasm` instantiate/link path |

## What Was Updated

### lp-engine

- `CraneliftGraphics` now uses `CraneliftEngine` trait instead of `jit()`
- `CraneliftShader` holds `CraneliftModule` instead of `JitModule`

### AGENTS.md

- Updated architecture diagram to show LPVM trait abstraction
- Added `lpvm` core traits crate to key crates table
- Added section explaining `LpvmEngine`/`LpvmModule`/`LpvmInstance`

## Final Architecture

```
GLSL → lps-frontend → LPIR → [LpvmEngine → LpvmModule → LpvmInstance]
                                      │
        ┌─────────────┼─────────────┐
        ▼             ▼             ▼
   lpvm-cranelift  lpvm-emu    lpvm-wasm
   (JIT)           (RISC-V emu) (wasmtime)
```

## Validation Results

All validation passed:
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server` ✓
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu` ✓
- `cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu` ✓
- `cargo test -p lps-filetests` (all backends) ✓
- `cargo +nightly fmt --check` ✓
- No warnings in affected crates

## Lines Changed

- Deleted: ~600-800 lines (legacy crates, old APIs)
- Modified: ~150-200 lines (migrations, documentation)
- Net: Significant reduction in code size and maintenance burden

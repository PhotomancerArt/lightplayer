# M3: `lpvm-cranelift` JIT

## Goal

Add LPVM trait implementations to the existing Cranelift crate. This validates
the trait API against a backend we have full control over and can optimize for.

**Key change from original plan:** We are NOT creating a new crate. We are:
1. Renaming `lpir-cranelift` → `lpvm-cranelift`
2. Adding trait implementations (`CraneliftEngine`, `CraneliftModule`, `CraneliftInstance`)
   alongside the existing API
3. Old API stays in place until M7 (when `lp-engine` migrates)

**Note**: `LpvmMemory` was not created as a separate trait — each backend
manages memory internally (wasmtime Store for WASM, JIT memory for Cranelift,
emulator RAM for RV32).

## Context for Agents

### Current `lpir-cranelift` architecture

`lpir-cranelift` is `#![no_std]` and works on both host (native ISA via
`cranelift-native`) and embedded (RISC-V, hardcoded triple).

**Key types:**

- `JitModule` — wraps `cranelift_jit::JITModule` + shader metadata + lookup
  tables. Created by `build_jit_module()`. Provides `call(name, args)` for
  Q32 typed calls and `direct_call(name)` for raw pointer access.
- `DirectCall` — raw function pointer + ABI metadata. Has `call_i32(vmctx, args)`
  and `call_i32_buf(vmctx, args, ret_buf)`. This is what the production engine
  render loop uses.
- `CompileOptions` — float mode, Q32 options, memory strategy, max errors.
- `CompileError`, `CompilerError`, `CallError`, `CallResult` — error types.

**Compile entry points:**

- `jit(source, options)` — GLSL → **`lps-frontend`** → LPIR → JitModule (needs
  frontend feature; name may still be `glsl` in code during migration)
- `jit_from_ir(ir, options)` — LPIR → JitModule (no metadata, limited call())
- `jit_from_ir_owned(ir, meta, options)` — LPIR + metadata → JitModule (full)

**How compilation works internally:**

1. Configure Cranelift flags (regalloc = single_pass, is_pic = false)
2. Build ISA — `cranelift_native` with `std`, else hardcoded `riscv32imac`
3. Create `JITBuilder` with builtin symbol lookup
4. Optionally use `AllocJitMemoryProvider` on embedded (no mmap)
5. Lower LPIR functions to Cranelift IR
6. `finalize_definitions()` — emit machine code
7. Return `JitModule` with metadata + code pointers

**How `DirectCall` works (the hot path):**

`DirectCall::call_i32_buf(vmctx, args, ret_buf)` calls `invoke_i32_args_returns`
which does a raw function pointer call. On ESP32, this is a direct call into
JIT-compiled RISC-V code in RAM. The VMContext pointer is passed as the first
argument.

**How `lp-engine` uses it:**

```
compile_shader:
  lpvm_cranelift::jit(source, options) → JitModule
  module.direct_call("main") → DirectCall

render (per pixel):
  direct_call.call_i32_buf(&vmctx, &args, &mut ret_buf)
```

**RV32 emulator code in `lpir-cranelift`:**

Behind `riscv32-emu` feature, `lpir-cranelift` also contains:

- `object_bytes_from_ir` — compile to RV32 object file
- `link_object_with_builtins` — link with builtins → ELF
- `glsl_q32_call_emulated` — run in emulator
- `run_lpir_function_i32` — convenience wrapper

This code stays in `lpvm-cranelift` but behind the `riscv32-emu` feature (M4).

## What To Build

### Crate location

`lp-shader/lpvm-cranelift/` (renamed from `lp-shader/legacy/lpir-cranelift/`)

### Dual API architecture

Both APIs coexist in the same crate:

**Old API (stays until M7):**
- `JitModule` — the existing monolithic type
- `DirectCall` — raw function pointer wrapper
- `GlslQ32`, `CallResult` — value marshaling
- `jit()`, `jit_from_ir()` — existing entry points

**New trait API (M3 adds this):**
- `CraneliftEngine` — implements `LpvmEngine`
- `CraneliftModule` — implements `LpvmModule`
- `CraneliftInstance` — implements `LpvmInstance`

### Module structure

```
lpvm-cranelift/
├── Cargo.toml
└── src/
    ├── lib.rs           # Re-exports both APIs
    ├── engine.rs        # CraneliftEngine: LpvmEngine implementation
    ├── module.rs        # CraneliftModule: LpvmModule implementation
    ├── instance.rs      # CraneliftInstance: LpvmInstance implementation
    ├── direct_call.rs   # DirectCall for hot-path access (existing)
    ├── call.rs          # Existing call infrastructure
    ├── values.rs        # GlslQ32, CallResult (existing)
    ├── compile.rs       # Existing JIT compilation
    ├── lower.rs         # Existing LPIR lowering
    ├── options.rs       # Existing CompileOptions
    └── error.rs         # Existing error types
```

### Trait implementation mapping

| LPVM trait     | Cranelift implementation           | Notes                                                                               |
|----------------|------------------------------------|-------------------------------------------------------------------------------------|
| `LpvmEngine`   | Configured Cranelift JIT builder   | Creates `JITModule`, holds compile options.                                         |
| `LpvmModule`   | Wraps finalized Cranelift JIT code | Immutable after compilation. Contains code pointers, metadata, function signatures. |
| `LpvmInstance` | VMContext pointer + call interface | Provides function calls with VMContext. Memory is internal to the JIT.              |

### The DirectCall question

The engine's hot path uses `DirectCall::call_i32_buf` — a raw function pointer
call. This is critical for performance.

The `LpvmInstance::call()` trait method (which takes `&str` name and `LpsValue`
args) adds overhead: name lookup, value marshaling. This is fine for filetests
but not for the render loop.

**Answer:** `DirectCall` stays as a separate method on `CraneliftModule` (beyond
the trait interface). The engine can choose which to use. No trait change needed.

### What to reuse from existing `lpir-cranelift`

All existing code stays in place. New trait implementations reuse:

- LPIR → Cranelift IR lowering
- JIT module building (ISA config, JITBuilder, finalize)
- Error types
- VMContext structure

## Unit Tests

- Compile a simple LPIR module via trait API
- Instantiate with memory
- Call a function via `LpvmInstance::call()`
- Verify return values
- Test VMContext passing (fuel, globals stub)
- Test the hot-path call mechanism (DirectCall) still works

## Performance Considerations

- The trait implementation MUST NOT add overhead to the hot path. The per-pixel
  render call (`call_i32_buf`) should be zero-cost compared to today's direct
  function pointer call.
- Use generics (monomorphization), not trait objects, for the engine path.
- Avoid allocations in the call path.

## What NOT To Do

- Do NOT remove the old API yet. It stays until M7.
- Do NOT require `std` for the core compilation path. Embedded JIT is the
  product — see AGENTS.md.
- Do NOT update `lp-engine` to use this yet. That's M6.
- Do NOT add `#[cfg(feature = "std")]` to any compile/execute path.

## Validation

```bash
# Must pass — embedded JIT is the product
cargo check -p lpvm-cranelift --target riscv32imac-unknown-none-elf

# Host build
cargo check -p lpvm-cranelift

# Tests (host only)
cargo test -p lpvm-cranelift
```

## Done When

- Crate renamed to `lpvm-cranelift`
- `LpvmEngine`/`LpvmModule`/`LpvmInstance` implemented
- LPIR → machine code compilation works via trait API
- Unit tests pass
- Compiles for `riscv32imac-unknown-none-elf` without `std`
- Hot-path call mechanism (`DirectCall`) is available and zero-overhead
- Trait API validated by two backends (WASM + Cranelift)
- Workspace builds pass

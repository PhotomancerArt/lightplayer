# M3: `lpvm-cranelift`

## Goal

Build the Cranelift JIT backend for LPVM. This should be mostly thin wrappers
around existing `lpir-cranelift` machinery, implementing the `LpvmModule`,
`LpvmInstance`, and `LpvmMemory` traits. Building this second (after WASM)
validates the trait API against a backend we have full control over.

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
  lpir_cranelift::jit(source, options) → JitModule
  module.direct_call("main") → DirectCall

render (per pixel):
  direct_call.call_i32_buf(&vmctx, &args, &mut ret_buf)
```

### RV32 emulator code in `lpir-cranelift`

Behind `riscv32-emu` feature, `lpir-cranelift` also contains:

- `object_bytes_from_ir` — compile to RV32 object file
- `link_object_with_builtins` — link with builtins → ELF
- `glsl_q32_call_emulated` — run in emulator
- `run_lpir_function_i32` — convenience wrapper

This code does NOT move to `lpvm-cranelift`. It belongs in `lpvm-rv32` (M4).
`lpvm-cranelift` is only the JIT backend.

## What To Build

### Crate location

`lpvm/lpvm-cranelift/`

### Cargo.toml structure

```toml
[package]
name = "lpvm-cranelift"
version = "0.1.0"
edition = "2024"

[dependencies]
lpvm = { path = "../lpvm", default-features = false }
lpir = { path = "../../lp-shader/lpir", default-features = false }
cranelift-codegen = { ..., default-features = false }
cranelift-frontend = { ..., default-features = false }
cranelift-jit = { ..., default-features = false }
cranelift-module = { ..., default-features = false }
lps-builtins = { ..., default-features = false }
# ... same cranelift deps as lpir-cranelift, minus riscv32-emu deps

[features]
default = ["std"]
std = ["cranelift-codegen/std", "cranelift-jit/std", "cranelift-native", ...]
cranelift-optimizer = ["cranelift-codegen/cranelift-optimizer"]
cranelift-verifier = ["cranelift-codegen/cranelift-verifier"]
```

**Critical**: this crate MUST work without `std` on `riscv32imac-unknown-none-elf`.
The `std` feature gates host-only things like `cranelift-native` (host ISA
autodetection). Without `std`, the target is hardcoded to RISC-V.

### Module structure

```
lpvm-cranelift/
├── Cargo.toml
└── src/
    ├── lib.rs           # Re-exports
    ├── module.rs        # CraneliftModule: LpvmModule implementation
    ├── instance.rs      # CraneliftInstance: LpvmInstance implementation
    ├── memory.rs        # CraneliftMemory: LpvmMemory implementation
    ├── compile.rs       # LPIR → Cranelift IR → machine code
    ├── lower.rs         # LPIR lowering to Cranelift IR (from lpir-cranelift)
    ├── call.rs          # Function call mechanics
    ├── direct_call.rs   # DirectCall for hot-path access
    ├── options.rs       # CompileOptions
    └── error.rs         # Error types
```

### Trait implementation mapping

| LPVM trait     | Cranelift implementation                        | Notes                                                                               |
|----------------|-------------------------------------------------|-------------------------------------------------------------------------------------|
| `LpvmModule`   | Wraps finalized Cranelift JIT code              | Immutable after compilation. Contains code pointers, metadata, function signatures. |
| `LpvmInstance` | VMContext + memory + call interface             | Owns or borrows LpvmMemory. Provides function calls with VMContext.                 |
| `LpvmMemory`   | Backing memory for VMContext + globals/uniforms | For JIT, this is the buffer that VMContext points into.                             |

### The DirectCall question

The engine's hot path uses `DirectCall::call_i32_buf` — a raw function pointer
call. This is critical for performance.

The `LpvmInstance::call()` trait method (which takes `&str` name and `LpvmValue`
args) adds overhead: name lookup, value marshaling. This is fine for filetests
but not for the render loop.

Options:

1. `LpvmInstance` has both `call(name, args)` (ergonomic) and a way to get a
   "prepared call" handle that avoids per-call overhead.
2. `lpvm-cranelift` exposes `DirectCall` as an additional type beyond the trait
   interface, and the engine uses it directly.
3. The trait has an associated type for "call handle" that backends can optimize.

The design chosen in M1 should address this. If it doesn't, this milestone will
surface the issue.

### What to extract from `lpir-cranelift`

Most of the compilation logic moves to `lpvm-cranelift`:

- LPIR → Cranelift IR lowering
- JIT module building (ISA config, JITBuilder, finalize)
- Function call mechanics (invoke, arg marshaling)
- CompileOptions, error types

What stays in `lpir-cranelift` (or moves to `lpvm-rv32`):

- `riscv32-emu` feature code: object compilation, ELF linking, emulated calls
- These are RV32 emulator concerns, not JIT concerns

What stays in `lpir-cranelift` temporarily:

- Anything that other crates still depend on, until they're migrated

## Unit Tests

- Compile a simple LPIR module via Cranelift
- Instantiate with memory
- Call a function via the trait interface
- Verify return values
- Test VMContext passing (fuel, globals stub)
- Test the hot-path call mechanism (DirectCall or equivalent)

## Performance Considerations

- The trait implementation MUST NOT add overhead to the hot path. The per-pixel
  render call (`call_i32_buf`) should be zero-cost compared to today's direct
  function pointer call.
- Use generics (monomorphization), not trait objects, for the engine path.
- Avoid allocations in the call path.

## What NOT To Do

- Do NOT move the `riscv32-emu` feature code into this crate. That's `lpvm-rv32`.
- Do NOT require `std` for the core compilation path. Embedded JIT is the
  product — see AGENTS.md.
- Do NOT delete `lpir-cranelift` yet. It coexists until consumers are migrated.
- Do NOT update `lp-engine` to use this yet. That's M6.
- Builtin crate path may still read `lps-builtins` on disk until rename
  completes; use workspace reality.
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

- `lpvm-cranelift` crate exists at `lpvm/lpvm-cranelift/`
- `LpvmModule`/`LpvmInstance`/`LpvmMemory` implemented
- LPIR → machine code compilation works
- Unit tests pass
- Compiles for `riscv32imac-unknown-none-elf` without `std`
- Hot-path call mechanism is available and zero-overhead
- Trait API validated by two backends (WASM + Cranelift) — any issues resolved
- Workspace builds pass

# Light Player Shader

`lp-shader` is the shader compiler and runtime for **LightPlayer** — a system that JIT-compiles
shader code to native code on resource-constrained devices at runtime.

The primary target is **RISC-V 32-bit** (`riscv32imac`, ESP32-C6), but the compiler supports any
ISA that Cranelift supports. A separate **WebAssembly** backend produces `.wasm` modules for
browser-based preview without pulling in Cranelift.

Floating point math is replaced with `Q16.16` (Q32) fixed-point math for performance on non-float
systems.

All compilation happens **on device** — there is no host cross-compilation step. The entire stack
is `no_std` + alloc.

## Pipeline

```
GLSL source (#version 450 core)
  │
  ▼
lps-frontend         Naga glsl-in → IrModule
  │
  ▼
LPIR                  flat, scalarized, mode-agnostic IR
  │
  ├──► lpvm-cranelift   → native machine code (RISC-V / host JIT)
  ├──► lps-wasm     → .wasm (browser preview, wasm.q32 filetests)
  └──► lpir::interp     → in-process interpreter (testing)
```

LPIR is LightPlayer's own intermediate representation. It keeps the compiler decoupled from
Cranelift — see `[lpir/README.md](lpir/README.md)` for rationale and examples, and
`[docs/design/lpir/](../docs/design/lpir/)` for the full spec.

## Q32 fixed-point mode

GLSL `float` can be compiled as **Q16.16 fixed-point** (`i32`) instead of IEEE `f32`. Q32 mode
targets hardware without an FPU (like ESP32-C6) and produces bit-identical results across the
native and WASM backends. Builtins (`sin`, `cos`, `sqrt`, etc.) are provided as `extern "C"`
functions in `lps-builtins` and linked at JIT time.

**Normative semantics** for Q32 (arithmetic, div-by-zero, relational builtins, filetest policy) are
in `[docs/design/q32.md](../docs/design/q32.md)`.

Float mode selection is a backend parameter — the IR itself is mode-agnostic.

## Crate index

**Full table:** `[CRATES.md](CRATES.md)`.

### Frontend and IR

- `lps-frontend/` — GLSL parsing (Naga) and lowering to LPIR
- `lpir/` — LightPlayer IR (types, ops, builder, parser, printer, interpreter, validator)
- `lps-shared/`, `lps-diagnostics/`, `lpvm/` — shared types and errors for
tests / exec helpers

### Q32 Fixed-Point Types

- `lps-q32/` — `Q32` fixed-point scalar, vector/matrix types (`Vec2Q32`–`Vec4Q32`,
`Mat2Q32`–`Mat4Q32`), component-wise helpers, encode/decode for compiler constants

### Codegen

- `lpvm-cranelift/` — LPIR → Cranelift → machine code (RISC-V on device, host JIT with `std`)
- `lps-wasm/` — LPIR → WASM (browser preview, `wasm.q32` filetests)

### Builtins

- `lps-builtin-ids/` — `BuiltinId` enum and mappings (generated)
- `lps-builtins/` — `extern "C"` builtin implementations (Q32 / f32, LPFX)
- `lps-builtins-gen-app/` — Scans builtins; emits IDs, ABI, refs, WASM import types
- `lps-builtins-emu-app/` — RISC-V guest linking all builtins (emulator filetests)
- `lps-builtins-wasm/` — WASM `cdylib` of builtins (`import-memory`)
- `lpfx-impl-macro/` — Proc-macros for LPFX builtin definitions

### Testing

- `lps-filetests/` — Cranelift-style GLSL filetests (JIT, RV32, WASM targets)
- `lps-filetests-gen-app/` — Generator for repetitive vector/matrix tests
- `lps-filetests-app/` — Filetest runner CLI
- `lps-exec/` — `GlslExecutable` trait; backend glue for filetests

### Browser demo

In-browser GLSL → WASM demo (workspace root `lp-app/web-demo/`): `just web-demo` from repo root.

## Running filetests

```bash
# Default backend (jit.q32)
./scripts/glsl-filetests.sh

# Specific backend
./scripts/glsl-filetests.sh --target wasm.q32
./scripts/glsl-filetests.sh --target rv32.q32

# Full matrix (same as CI)
just test-filetests
```

See `scripts/glsl-filetests.sh --help` for targets, filters, and thread control.

```bash
# Run via cargo (jit.q32 only)
cargo run -p lps-filetests-app --bin lps-filetests-app -- test "*add*"
cargo test -p lps-filetests --test filetests -- --ignored
```

## Building

```bash
# Host (default members)
cargo build

# Firmware check (on-device compiler included)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Full CI build (host + RV32 builtins + emu guest)
just build-ci

# Fix, check, and test the whole GLSL stack
just fci-glsl
```


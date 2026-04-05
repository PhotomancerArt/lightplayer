# Light Player Shader

`lp-shader` is the shader compiler for **LightPlayer** — a system that JIT-compiles shader code to
native code on resource-constrained devices at runtime.

The primary target is **RISC-V 32-bit** (`riscv32imac`, ESP32-C6), but the compiler supports any
ISA that Cranelift supports. A separate **WebAssembly** backend produces `.wasm` modules for
browser-based preview without pulling in Cranelift.

All compilation happens **on device** — there is no host cross-compilation step. The entire stack
is `no_std` + alloc.

## Pipeline

```
GLSL source (#version 450 core)
  │
  ▼
lp-glsl-naga         Naga glsl-in → IrModule
  │
  ▼
LPIR                  flat, scalarized, mode-agnostic IR
  │
  ├──► lpir-cranelift   → native machine code (RISC-V / host JIT)
  ├──► lp-glsl-wasm     → .wasm (browser preview, wasm.q32 filetests)
  └──► lpir::interp     → in-process interpreter (testing)
```

LPIR is LightPlayer's own intermediate representation. It keeps the compiler decoupled from
Cranelift — see [`lpir/README.md`](lpir/README.md) for rationale and examples, and
[`docs/design/lpir/`](../docs/design/lpir/) for the full spec.

## Q32 fixed-point mode

GLSL `float` can be compiled as **Q16.16 fixed-point** (`i32`) instead of IEEE `f32`. Q32 mode
targets hardware without an FPU (like ESP32-C6) and produces bit-identical results across the
native and WASM backends. Builtins (`sin`, `cos`, `sqrt`, etc.) are provided as `extern "C"`
functions in `lp-glsl-builtins` and linked at JIT time.

**Normative semantics** for Q32 (arithmetic, div-by-zero, relational builtins, filetest policy) are
in [`docs/design/q32.md`](../docs/design/q32.md).

Float mode selection is a backend parameter — the IR itself is mode-agnostic.

## Crate index

**Full table:** [`CRATES.md`](CRATES.md).

### Frontend and IR

- `lp-glsl-naga/` — GLSL parsing (Naga) and lowering to LPIR
- `lpir/` — LightPlayer IR (types, ops, builder, parser, printer, interpreter, validator)
- `lps-shared/`, `lp-glsl-diagnostics/`, `lpvm/` — shared types and errors for
  tests / exec helpers

### Codegen

- `lpir-cranelift/` — LPIR → Cranelift → machine code (RISC-V on device, host JIT with `std`)
- `lp-glsl-wasm/` — LPIR → WASM (browser preview, `wasm.q32` filetests)

### Builtins

- `lp-glsl-builtin-ids/` — `BuiltinId` enum and mappings (generated)
- `lp-glsl-builtins/` — `extern "C"` builtin implementations (Q32 / f32, LPFX)
- `lp-glsl-builtins-gen-app/` — Scans builtins; emits IDs, ABI, refs, WASM import types
- `lp-glsl-builtins-emu-app/` — RISC-V guest linking all builtins (emulator filetests)
- `lp-glsl-builtins-wasm/` — WASM `cdylib` of builtins (`import-memory`)
- `lpfx-impl-macro/` — Proc-macros for LPFX builtin definitions

### Testing

- `lp-glsl-filetests/` — Cranelift-style GLSL filetests (JIT, RV32, WASM targets)
- `lp-glsl-filetests-gen-app/` — Generator for repetitive vector/matrix tests
- `lp-glsl-filetests-app/` — Filetest runner CLI
- `lp-glsl-exec/` — `GlslExecutable` trait; backend glue for filetests

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
cargo run -p lp-glsl-filetests-app --bin lp-glsl-filetests-app -- test "*add*"
cargo test -p lp-glsl-filetests --test filetests -- --ignored
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

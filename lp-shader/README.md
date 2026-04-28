# Light Player Shader

`lp-shader` is the shader compiler and runtime for **LightPlayer** — a system that JIT-compiles
shader code to native code on resource-constrained devices at runtime.

The primary target is **RISC-V 32-bit** (`riscv32imac`, ESP32-C6). Two native backends are
available: `lpvm-native` (custom lightweight codegen, default on-device) and `lpvm-cranelift`
(Cranelift-based, reference implementation). A separate **WebAssembly** backend produces `.wasm`
modules for browser-based preview.

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
  ├──► lpvm-native      → native machine code (default on-device JIT)
  ├──► lpvm-cranelift    → native machine code (Cranelift, reference backend)
  ├──► lpvm-wasm         → .wasm (browser preview, wasm.q32 filetests)
  └──► lpir::interp      → in-process interpreter (testing)
```

LPIR is LightPlayer's own intermediate representation. It keeps the compiler decoupled from
any specific backend — see `[lpir/README.md](lpir/README.md)` for rationale and examples, and
`[docs/design/lpir/](../docs/design/lpir/)` for the full spec.

## Q32 fixed-point mode

GLSL `float` can be compiled as **Q16.16 fixed-point** (`i32`) instead of IEEE `f32`. Q32 mode
targets hardware without an FPU (like ESP32-C6) and produces bit-identical results across the
native and WASM backends. Builtins (`sin`, `cos`, `sqrt`, etc.) are provided as `extern "C"`
functions in `lps-builtins` and linked at JIT time.

**Normative semantics** for Q32 (arithmetic, div-by-zero, relational builtins, filetest policy) are
in `[docs/design/q32.md](../docs/design/q32.md)`.

Float mode selection is a backend parameter — the IR itself is mode-agnostic.

## Texture reads (`sampler2D`)

Shaders may declare `sampler2D` uniforms and call `texelFetch` and `texture`.
Compile-time policy (`TextureBindingSpec`, keyed by uniform name) and runtime
buffers are wired outside GLSL — see `CompilePxDesc::with_texture_spec`,
`lp_shader::texture_binding::{texture2d, height_one}`, and
`LpsTextureBuf::{to_texture2d_value, to_named_texture_uniform}` in `lp-shader`.

Full contract (formats, wraps/filters, guest vs host metadata, filetests,
follow-ups): [`docs/design/lp-shader-texture-access.md`](../docs/design/lp-shader-texture-access.md).

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

- `lpvm-native/` — LPIR → custom RV32 machine code (default on-device JIT, pool-based regalloc)
- `lpvm-cranelift/` — LPIR → Cranelift → machine code (reference backend, host JIT with `std`)
- `lpvm-wasm/` — LPIR → WASM (browser preview, `wasm.q32` filetests)

### Builtins

- `lps-builtin-ids/` — `BuiltinId` enum and mappings (generated)
- `lps-builtins/` — `extern "C"` builtin implementations (Q32 / f32, LPFX)
- `lps-builtins-gen-app/` — Scans builtins; emits IDs, ABI, refs, WASM import types
- `lps-builtins-emu-app/` — RISC-V guest linking all builtins (emulator filetests)
- `lpfn-impl-macro/` — Proc-macros for LPFX builtin definitions

### Testing

- `lps-filetests/` — GLSL filetests (JIT, RV32 native, RV32 Cranelift, WASM targets)
- `lps-filetests-gen-app/` — Generator for repetitive vector/matrix tests
- `lps-filetests-app/` — Filetest runner CLI

### Browser demo

In-browser GLSL → WASM demo (workspace root `lp-app/web-demo/`): `just web-demo` from repo root.

## Running filetests

```bash
# Default targets (rv32n.q32, rv32c.q32, etc.)
./scripts/filetests.sh

# Specific backend
./scripts/filetests.sh --target wasm.q32
./scripts/filetests.sh --target rv32n.q32
./scripts/filetests.sh --target rv32c.q32

# Full matrix (same as CI)
just test-filetests
```

See `scripts/filetests.sh --help` for targets, filters, and thread control.

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

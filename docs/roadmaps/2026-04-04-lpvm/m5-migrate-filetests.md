# M5: Migrate Filetests

## Goal

Port **`lps-filetests`** (rename of `lp-glsl-filetests`) from **`GlslExecutable`**
to the LPVM trait system. All three backends (JIT, RV32, WASM) exercise the same
LPVM-shaped API. Primary validation step.

## Naming / paths

Target crate: **`lps-filetests`**. During migration the directory or package may
still be `lp-glsl-filetests`; use **`cargo test -p <actual-package-name>`**.

## Context for Agents

### How filetests work today

1. Read `.glsl` tests + expected outputs.
2. Select backend: `Backend::Jit` | `Rv32` | `Wasm`.
3. Build **`Box<dyn GlslExecutable>`** (or equivalent).
4. Call functions, compare to expectations.

Typical wiring:

- **Jit** → `LpirJitExecutable` + **`lpir_cranelift::JitModule`**
- **Rv32** → object + link + emulate (`lpir-cranelift` `riscv32-emu` path today)
- **Wasm** → emission + wasmtime (runner may live in filetests until **`lpvm-wasm`**
  `runtime` is ready)

### `GlslExecutable` surface

Typed `call_*`, `call_array`, `get_function_signature`, `list_functions`, plus
optional std debug hooks (`format_clif_ir`, etc.). Logical signatures use
**`lps-shared`** (`LpsFunctionSignature`, etc.) after M1 renames.

### Target shape

1. Compile / load → **`LpvmModule`** (per backend crate).
2. **`LpvmInstance`** + **`LpvmMemory`** as designed in M1–M4.
3. Calls through LPVM API; keep **ergonomic test helpers** (wrapper module or
   extension traits) so tests stay readable.

### Debug output

Backend-specific formatting (CLIF, VCode, wasm WAT, emulator state) may **not**
live on the core traits. Expose via concrete backend types or side APIs that
filetests import explicitly.

## Migration strategy

1. **Parallel path**: keep `GlslExecutable` working while LPVM path is built.
2. **Switch** when LPVM path passes all tests on all backends.
3. **Remove** `GlslExecutable` and `lp-glsl-exec` dependency from filetests.

## Dependencies after migration (illustrative)

```toml
[dependencies]
lpvm = { path = "../../lpvm/lpvm" }
lpvm-cranelift = { path = "../../lpvm/lpvm-cranelift" }
lpvm-rv32 = { path = "../../lpvm/lpvm-rv32" }
lpvm-wasm = { path = "../../lpvm/lpvm-wasm", features = ["runtime"] }
lpir = { path = "../lpir" }   # or top-level path after moves
lps-shared = { path = "../lps-shared" }
lps-naga = { path = "../lps-naga" }
```

**Remove** (once fully migrated): `lp-glsl-exec`, direct `lpvm`, direct
`lpir-cranelift` (replaced by `lpvm-cranelift` / `lpvm-rv32`), direct `wasmtime`
if folded into `lpvm-wasm` runtime.

## What NOT To Do

- Do NOT drop a backend.
- Do NOT weaken debug output without replacement.
- Do NOT change test expectations to hide bugs (project rule).

## Done When

- All filetests pass on **Jit, RV32, WASM** via LPVM
- `GlslExecutable` unused in filetests
- **`lps-filetests`** (or current package name) depends on `lpvm` + three
  backends, not on `lp-glsl-exec`

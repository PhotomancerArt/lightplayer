# lps-builtins

Low-level builtin library for **LightPlayer JIT shaders**: fixed-point and float math, memory
helpers, and host hooks. Symbols are exported as `#[no_mangle] pub extern "C"` so
`lpir-cranelift` can link them into generated RISC-V code (and the RISC-V / WASM test harnesses
can resolve the same names).

## Layout

- **`src/builtins/glsl/`** — GLSL scalar builtins (mostly `*_q32.rs`)
- **`src/builtins/lpir/`** — LPIR helper ops (e.g. `fsqrt_q32`)
- **`src/builtins/lpfx/`** — LightPlayer extension / generative functions (LPFX macros via
  `lpfx-impl-macro`)
- **`src/glsl/q32/`** — Q32 vector/matrix types and small helpers used by builtins
- **`src/mem.rs`** — `memcpy` / `memset` / `memcmp` for `no_std`
- **`src/host/`** — Debug / host interface when `std` or logging is enabled

## Wiring into the compiler

Builtin **IDs** and Cranelift **ABI tables** are not edited by hand. Run
**`lps-builtins-gen-app`** (or `scripts/build-builtins.sh`), which scans `src/builtins/` and
writes:

- `lps-builtin-ids` (`lib.rs`, `glsl_builtin_mapping.rs`)
- `lpir-cranelift/src/generated_builtin_abi.rs`
- `lps-builtins-emu-app` / `lps-builtins-wasm` `builtin_refs.rs`
- `lps-builtins/src/builtins/glsl/mod.rs` and `lpir/mod.rs` (module lists)
- `lps-wasm/src/emit/builtin_wasm_import_types.rs`

## Adding a builtin

1. Add the implementation under `src/builtins/` (follow existing patterns in `glsl/`, `lpir/`, or
   `lpfx/`).
2. Regenerate boilerplate:

   ```bash
   cargo run -p lps-builtins-gen-app
   ```

   or from repo root:

   ```bash
   scripts/build-builtins.sh
   ```

3. Rebuild RV32 emu app / WASM builtins if you need those artifacts (`just build-rv32-builtins`,
   `scripts/build-builtins.sh`, etc.).

## Dependency

```toml
[dependencies]
lps-builtins = { path = "../lps-builtins", default-features = false }
```

Path is relative to your crate; from another top-level crate use
`path = "lp-shader/lps-builtins"`.

## RISC-V guest binary

`lps-builtins-emu-app` links every builtin so the emulator-based filetests can resolve symbols.
See that crate’s README and `scripts/build-builtins.sh`.

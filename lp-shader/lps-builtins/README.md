# lps-builtins

Low-level builtin library for **LightPlayer JIT shaders**: fixed-point and float math, memory
helpers, and host hooks. Symbols are exported as `#[no_mangle] pub extern "C"` so the native and
Cranelift backends can link them into generated RISC-V code (and the RISC-V / WASM test harnesses
can resolve the same names).

## Layout

- **`src/builtins/glsl/`** — GLSL scalar builtins (mostly `*_q32.rs`)
- **`src/builtins/lpir/`** — LPIR helper ops (e.g. `fsqrt_q32`)
- **`src/builtins/lpfn/`** — LightPlayer extension / generative functions (LPFX macros via
  `lpfn-impl-macro`)
- **`glsl/lpfn/`** — **canonical GLSL sources** for the lpfn builtins (see below)
- **`src/canonical_glsl.rs`** — manifest embedding the canonical sources (`include_str!`)
- **`src/glsl/q32/`** — Q32 vector/matrix types and small helpers used by builtins
- **`src/mem.rs`** — `memcpy` / `memset` / `memcmp` for `no_std`
- **`src/host/`** — Debug / host interface when `std` or logging is enabled

## Canonical GLSL sources

GLSL is the canonical source of truth for lpfn builtin **float** semantics
(`docs/adr/2026-07-08-glsl-canonical-builtins.md`). Each lpfn builtin has one
`.glsl` file under `glsl/lpfn/` mirroring the Rust layout; the files are
float+integer GLSL ports of the algorithms the Q32 Rust files implement
(same integer hashes and structure, ideal-precision constants), and they use
the real `lpfn_*` names so the GPU preview path can splice them into shaders
as a prelude.

The Rust `*_q32.rs` implementations are **device approximations** of these
sources, held to per-builtin tolerances by the conformance suite in
`lps-filetests` (`src/conformance/`): the canonical GLSL is compiled with
`lps-frontend` and interpreted natively in f32 (the oracle), the Q32
builtins are invoked through a compiled `wasm.q32` probe shader, and the two
are compared pointwise (integer-hash noise, color, math) or statistically
(the chaotic sin-hash random family). Run it with:

```bash
cargo test -p lps-filetests conformance -- --nocapture
```

The `*_f32.rs` variants are legacy stubs (they convert to Q32 and call the
Q32 impl) and are not a semantic reference.

When adding or changing an lpfn builtin, update the canonical `.glsl`, the
`canonical_glsl.rs` manifest entry, and the conformance spec
(`lps-filetests/src/conformance/spec.rs`) together with the Rust
implementation. Note: `lps-frontend` reserves the `lpfn_` prefix for builtin
imports, so harnesses that compile the canonical sources through the normal
frontend rename the prefix first (see `conformance/oracle.rs`).

## Wiring into the compiler

Builtin **IDs** and **ABI tables** are not edited by hand. Run
**`lps-builtins-gen-app`** (or `scripts/build-builtins.sh`), which scans `src/builtins/` and
writes:

- `lps-builtin-ids` (`lib.rs`, `glsl_builtin_mapping.rs`)
- `lpvm-cranelift/src/generated_builtin_abi.rs`
- `lps-builtins-emu-app` / `lps-builtins-wasm` `builtin_refs.rs`
- `lps-builtins/src/builtins/glsl/mod.rs` and `lpir/mod.rs` (module lists)
- `lpvm-wasm/src/emit/builtin_wasm_import_types.rs`

## Adding a builtin

1. Add the implementation under `src/builtins/` (follow existing patterns in `glsl/`, `lpir/`, or
   `lpfn/`).
2. Regenerate boilerplate:

   ```bash
   cargo run -p lps-builtins-gen-app
   ```

   or from repo root:

   ```bash
   scripts/build-builtins.sh
   ```

3. Rebuild RV32 emu app / WASM builtins if you need those artifacts (`just build-rv32c-builtins`,
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

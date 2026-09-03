# lps-builtins

Low-level builtin library for **LightPlayer JIT shaders**: fixed-point and float math, memory
helpers, and host hooks. Symbols are exported as `#[no_mangle] pub extern "C"` so the native and
Cranelift backends can link them into generated RISC-V code (and the RISC-V / WASM test harnesses
can resolve the same names).

## Layout

- **`src/builtins/glsl/`** — GLSL scalar builtins (`*_q32.rs` + `*_f32.rs`)
- **`src/builtins/lpir/`** — LPIR helper ops (e.g. `fsqrt_q32` / `fsqrt_f32`)
- **`src/builtins/lpfn/`** — LightPlayer extension / generative functions (LPFX macros via
  `lpfn-impl-macro`)
- **`src/builtins/texture/`** — sampler entry points and the sampling reference math
- **`src/f32_math.rs`** — `no_std` f32 primitives shared by the native-f32 family
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

When adding or changing an lpfn builtin, update the canonical `.glsl`, the
`canonical_glsl.rs` manifest entry, and the conformance spec
(`lps-filetests/src/conformance/spec.rs`) together with the Rust
implementation. Note: `lps-frontend` reserves the `lpfn_` prefix for builtin
imports, so harnesses that compile the canonical sources through the normal
frontend rename the prefix first (see `conformance/oracle.rs`).

## The native-f32 family (`float-f32`)

Every builtin exists twice: `*_q32` for Fixed mode and `*_f32` for Float mode,
plus a handful (`lpfn_hash_*`, `__lp_vm_get_fuel`) whose ABI carries no float
and which serve both. The f32 half covers the `glsl` transcendentals, the
`lpir` library ops, the `lpfn` generative/color library, and the `texture`
samplers.

**Semantics are governed by [`docs/design/float.md`](../../docs/design/float.md).**
Its §3 Guaranteed rows (`+ - * /`, `sqrt`, comparisons, conversions,
`floor`/`ceil`/`trunc`) are implemented as the native operation, exactly — the
f32 builtins do **not** inherit the Q32 approximations. §6 governs the rest:
builtins are approximations of the canonical GLSL within a documented
tolerance, and **every f32 file states its band in its module docs**. Two
deliberate deviations exist and say so:

| Builtin | Deviation | Band |
|---|---|---|
| `inversesqrt` | one Newton step from the bit-trick seed instead of `1/sqrt(x)` | 2e-3 relative |
| `lpir::fdiv_recip` | `a * (1/b)` — a second rounding, mirroring the Q32 reciprocal mode | 2 ulp |

Everything else delegates to `libm`: in f32 the accurate implementation is also
the cheap one, so speed-over-ulp is a licence rather than an obligation.

§5 Unspecified inputs (`asin(2)`, `log(-1)`, `normalize(0)`, NaN through
`min`/`max`) return *something*, never trap, and are **never asserted** — not
here and not in the corpus.

Two operations that are easy to conflate are spelled separately: GLSL
`round()` (`glsl/round_f32.rs`) rounds ties **away from zero**, matching the Q32
sibling and `interp.f32`; `fnearest` (`lpir/fnearest_f32.rs`) rounds ties **to
even**, matching wasm's `f32.nearest`.

The family is behind the **`float-f32`** feature, off by default. `FloatMode` is
matched on a runtime value, so LTO cannot drop the family on its own and a
Fixed-only device image would pay for code it never calls. Enable it where f32
shaders actually run; the rv32 builtins image
(`lps-builtins-emu-app`) enables it because it is the host-side oracle and
because `lpvm-cranelift`'s linker asserts every `BuiltinId` symbol is present.

Note what the feature does **not** cover: `lps-builtin-ids` is a separate crate
the compiler always links, and its generated name/lookup tables grow with every
`BuiltinId` variant regardless of this flag.

## Cycle census and bit-identity proofs

Two instruments exist for the hot Q32 math builtins
(`docs/reports/2026-09-02-q32-builtins-cycles.md`):

- **Cycle census** — cycles per call on the RV32 emulator (`CycleModel::Esp32C6`)
  for `exp`, `sqrt`, `inversesqrt`, `sin`, `cos`, `/`, `mod`, wrapper overhead
  subtracted. Lives in `lps-filetests` (`src/test_run/builtin_cycle_census.rs`):

  ```bash
  scripts/build-builtins.sh
  cargo test -p lps-filetests --release builtin_cycle_census -- --ignored --nocapture
  # LP_CENSUS_DETAIL=1 prints every (input → cycles) sample
  ```

  The census runs on the filetests image (`opt-level=1`); the profiler and the
  device compile this crate at `opt-level=3`, so use it to rank and to measure
  deltas, and `lp-cli profile function` for absolute numbers on a workload.

- **Bit-identity proofs** — `exp_q32.rs` and `fsqrt_q32.rs` keep their former
  implementation as a test-only reference and prove the shipped code equal for
  every `i32` input. The exhaustive proofs are `#[ignore]` (seconds in release);
  sampled versions run in the default suite:

  ```bash
  cargo test -p lps-builtins --release exhaustive -- --ignored --nocapture
  ```

  A Q32 builtin rewrite that is not bit-identical is an accuracy decision:
  ADR + re-blessed expectations on every Q32 filetest target
  (`docs/design/q32.md`).

## Wiring into the compiler

Builtin **IDs** and **ABI tables** are not edited by hand. Run
**`lps-builtins-gen-app`** (or `scripts/build-builtins.sh`), which scans `src/builtins/` and
writes:

- `lps-builtin-ids` (`lib.rs`, `glsl_builtin_mapping.rs`)
- `lpvm-cranelift/src/generated_builtin_abi.rs`
- `lps-builtin-ids` (`lib.rs`, `glsl_builtin_mapping.rs` — per-mode resolvers and
  the mode-taking facades every backend should call)
- `lps-builtins-emu-app` / `lps-builtins-wasm` `builtin_refs.rs`
- `lps-builtins/src/jit_builtin_ptr.rs` (`BuiltinId` → code address)
- `lps-builtins/src/builtins/glsl/mod.rs`, `lpir/mod.rs`, `vm/mod.rs` (module lists)
- `lpvm-wasm/src/emit/builtin_wasm_import_types.rs`
- `lpvm-wasm/src/rt_wasmtime/native_builtin_dispatch.rs`

Generated files carry `#[cfg(feature = "float-f32")]` on the f32 entries, so a
crate that re-exports one (like `lps-builtins-emu-app`) needs its own
`float-f32` feature forwarding to this one — the `cfg` is evaluated against the
*consuming* crate's features.

**Resolvers never cross modes.** In Float mode only f32 (and mode-independent)
ids resolve, with no fallback to Q32. A builtin taking a vector receives a
pointer, both sides are `i32`, and a Q32 builtin handed an f32 module's memory
reinterprets f32 bit patterns as Q16.16 — wrong answers with no type error
anywhere. That rule is pinned by unit tests in both `lps-builtin-ids` and
`lpvm-wasm/src/emit/imports.rs`.

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

# Summary — Q32Options dispatch (2026-04-18)

## What changed

- `lpir::CompilerConfig` gained a `q32: Q32Options` field; both
  `NativeCompileOptions` and `WasmOptions` thread it via `.config`.
- Three new `compile-opt` filetest keys: `q32.add_sub`, `q32.mul`,
  `q32.div`. Each accepts `saturating` (default), `wrapping` (add/sub
  and mul), or `reciprocal` (div).
- `lps-builtins` gained `__lp_lpir_fdiv_recip_q32` (port of deleted
  `lp-glsl/.../div_recip.rs`, with new explicit `divisor == 0`
  saturation guard matching `__lp_lpir_fdiv_q32` policy).
- `lps-q32` gained `FromStr` impls for `AddSubMode`, `MulMode`,
  `DivMode` consumed by `CompilerConfig::apply`.
- `lpvm-native`: introduced `LowerOpts<'a>` carrier struct;
  `lower_lpir_op` now dispatches `Fadd`/`Fsub`/`Fmul`/`Fdiv` based on
  `opts.q32`. Wrapping `Fadd`/`Fsub` lower to a single `AluRRR { Add | Sub }`
  VInst; wrapping `Fmul` is a 5-VInst `mul/mulh/srli/slli/or` sequence
  (new `AluOp::MulH` variant + RV32 `mulh` encoding); reciprocal `Fdiv`
  sym_calls the new helper via `BuiltinId::LpLpirFdivRecipQ32`.
- `lpvm-wasm`: `EmitCtx` carries `q32`; new
  `emit_q32_{fadd,fsub,fmul}_wrap` helpers (1, 1, 6 wasm ops respectively)
  and `emit_q32_fdiv_recip` (~15 wasm ops with 7 scratch i32 locals).
  Bit-identical to native's reciprocal helper by construction (verified
  via wasmtime cross-check test against the actual Phase 2 helper).
- `lp-engine` gfx glue: `gfx/native_jit.rs` and `gfx/cranelift.rs` now
  set `config.q32 = options.q32_options` instead of silently dropping it.
  (Note: `gfx/native_object.rs` does not exist in this repo — the only
  two glue sites were updated.)
- `lp-engine` Cargo.toml: `cranelift` feature now also enables `lpir`,
  needed because `gfx/cranelift.rs` references `lpir::CompilerConfig`
  unconditionally.
- 4 new filetests under `lp-shader/lps-filetests/filetests/scalar/float/`:
  `q32fast-add-sub.glsl`, `q32fast-mul.glsl`, `q32fast-div-recip.glsl`,
  `q32fast-div-recip-by-zero.glsl`.
- Documentation: `lower.rs` Q32 op lowering policy header updated to
  reflect the new dispatch buckets; `fadd_q32.rs`/`fsub_q32.rs`/
  `fmul_q32.rs`/`fdiv_q32.rs` doc comments note that backends inline a
  faster expansion when the shader opts into the relevant fast mode;
  `q32_options.rs` doc comment explains it's the source of truth wired
  through `lpir::CompilerConfig`.

## What did not change

- Defaults across `add_sub`, `mul`, `div` remain `Saturating` — existing
  shaders compile to identical code.
- `ShaderCompileOptions::q32_options` (lp-engine top-level) and cranelift's
  `CompileOptions::q32_options` (top-level) remain for API stability and
  are kept in sync with `config.q32`.
- Cranelift codegen is unchanged: it still does not dispatch on Q32 mode
  (deprecated path). Filetests using `q32.*, wrapping` are marked
  `@unsupported(rv32c.q32)` to document this. The filetests still run
  and pass on `rv32n.q32` (native JIT) and `wasm.q32` (preview).

## Files touched

- `Cargo.lock` — dependency graph updates.
- `lp-cli/src/commands/shader_debug/collect.rs` — only other
  `lower_ops` caller; updated to pass `LowerOpts`.
- `lp-core/lp-engine/Cargo.toml` — `cranelift` feature now enables `lpir`.
- `lp-core/lp-engine/src/gfx/cranelift.rs` — sets `config.q32`.
- `lp-core/lp-engine/src/gfx/native_jit.rs` — sets `config.q32`; drops
  the `let _ = options.q32_options` discard.
- `lp-shader/lpir/Cargo.toml` — adds `lps-q32` dep.
- `lp-shader/lpir/src/compiler_config.rs` — `q32` field, `apply` arms,
  tests.
- `lp-shader/lps-builtin-ids/src/glsl_builtin_mapping.rs` — generated
  via `lps-builtins-gen-app` to include `LpLpirFdivRecipQ32`.
- `lp-shader/lps-builtin-ids/src/lib.rs` — generated.
- `lp-shader/lps-builtins-emu-app/src/builtin_refs.rs` — generated.
- `lp-shader/lps-builtins/src/builtin_refs.rs` — generated.
- `lp-shader/lps-builtins/src/builtins/lpir/fadd_q32.rs` — doc note.
- `lp-shader/lps-builtins/src/builtins/lpir/fdiv_q32.rs` — doc note.
- `lp-shader/lps-builtins/src/builtins/lpir/fdiv_recip_q32.rs` — **new**.
- `lp-shader/lps-builtins/src/builtins/lpir/fmul_q32.rs` — doc note.
- `lp-shader/lps-builtins/src/builtins/lpir/fsub_q32.rs` — doc note.
- `lp-shader/lps-builtins/src/builtins/lpir/mod.rs` — generated.
- `lp-shader/lps-builtins/src/jit_builtin_ptr.rs` — wired new helper
  pointer.
- `lp-shader/lps-q32/src/q32_options.rs` — `FromStr` impls + tests +
  header note.
- `lp-shader/lpvm-cranelift/src/compile_options.rs` — vestigial doc
  comment on `q32_options`.
- `lp-shader/lpvm-cranelift/src/generated_builtin_abi.rs` — generated.
- `lp-shader/lpvm-native/Cargo.toml` — adds `lps-q32` dep.
- `lp-shader/lpvm-native/src/compile.rs` — builds `LowerOpts` from
  session options.
- `lp-shader/lpvm-native/src/isa/rv32/emit.rs` — emit `MulH`.
- `lp-shader/lpvm-native/src/isa/rv32/encode.rs` — `encode_mulh`.
- `lp-shader/lpvm-native/src/lib.rs` — `mod lower_opts`, `pub use`.
- `lp-shader/lpvm-native/src/lower.rs` — Q32 dispatch + 8 unit tests +
  policy header refresh.
- `lp-shader/lpvm-native/src/lower_opts.rs` — **new**.
- `lp-shader/lpvm-native/src/vinst.rs` — `AluOp::MulH` + helpers.
- `lp-shader/lpvm-wasm/Cargo.toml` — adds `lps-q32` dep.
- `lp-shader/lpvm-wasm/src/emit/builtin_wasm_import_types.rs` — generated.
- `lp-shader/lpvm-wasm/src/emit/func.rs` — `FdivRecipLocals` allocation.
- `lp-shader/lpvm-wasm/src/emit/mod.rs` — `EmitCtx::q32`,
  `FuncEmitCtx::fdiv_recip_scratch`.
- `lp-shader/lpvm-wasm/src/emit/ops.rs` — Q32 dispatch.
- `lp-shader/lpvm-wasm/src/emit/q32.rs` — new `emit_q32_*_wrap` helpers
  and `emit_q32_fdiv_recip`.
- `lp-shader/lpvm-wasm/src/rt_wasmtime/native_builtin_dispatch.rs` —
  generated.
- `lp-shader/lpvm-wasm/tests/q32_options_dispatch.rs` — **new**
  integration tests (bytes + wasmtime cross-check vs. phase 2 helper).
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-add-sub.glsl` —
  **new**.
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-mul.glsl` —
  **new**.
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip.glsl` —
  **new**.
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip-by-zero.glsl` —
  **new**.

## Validation

- `cargo build` (default workspace members): clean.
  (`cargo build --workspace` is not runnable on macOS host because the
  ESP32 firmware crates `esp-rom-sys`, `esp-sync`, `esp32c6` don't build
  on host — pre-existing, unrelated.)
- `cargo test -p lps-q32 -p lpir -p lps-builtins -p lpvm-native -p lpvm-wasm
  -p lp-engine`: all green
  (~850 tests across the touched crates, including 5 new
  `fdiv_recip_q32` tests, 8 new `lower.rs` Q32 dispatch tests, and 15
  new `lpvm-wasm` integration tests).
- `TEST_FILE=q32fast cargo test -p lps-filetests --test filetests --
  --ignored`: all 4 new files pass on `rv32n.q32` and `wasm.q32` backends
  (12 expectation checks total). `rv32c.q32` is `@unsupported` for
  wrapping ops as documented.

## Known follow-ups

- **Cranelift Q32 dispatch.** Cranelift's RV32 backend does not yet
  thread `CompilerConfig::q32` (it ignores both top-level
  `CompileOptions::q32_options` and the new `config.q32`). The two
  wrapping filetests are marked `@unsupported(rv32c.q32)` to document
  this. Cranelift is the deprecated path (per AGENTS / project notes),
  so this is intentionally not addressed here.
- **Reciprocal `Fdiv` precision sweep.** The `q32fast-div-recip.glsl`
  filetest uses a single `10/3` case at 0.001 tolerance. A wider sweep
  characterizing edge-case precision (very small divisors, near-zero
  dividends, signs around `i32::MIN`) would strengthen confidence; not
  required for v1 since the Rust unit tests in `fdiv_recip_q32::tests`
  cover those.
- **Pre-existing filetest baseline.** A full
  `cargo test -p lps-filetests` run reports ~50 unrelated compile
  failures on various `builtins/*.glsl` files. These pre-date this
  work; the `q32fast-*` files all pass.

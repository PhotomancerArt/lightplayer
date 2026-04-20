# Phase 6 — Cleanup, validate, summary, commit

## Scope of phase

Final pass: documentation polish, full workspace validation, write
`summary.md`, move plan into `docs/plans-old/`, single git commit
covering phases 1-6.

This phase is run by the orchestrating agent (parent), not delegated to a
sub-agent — it spans many crates and needs to integrate findings from the
prior phase reports.

## Sub-agent Reminders (for any nested help)

- Do **not** make further code changes beyond what's listed below.
- Do **not** weaken tests.
- If validation surfaces a real bug from earlier phases, file it as a
  TODO in `summary.md` rather than silently fixing — we want a clean
  audit trail.

## Step 1: Documentation polish

### Update lpvm-native `lower.rs` module-level doc

Add a short note near the top of `lp-shader/lpvm-native/src/lower.rs`
explaining the dispatch policy (so future readers don't have to rediscover
it):

```rust
//! Lowering from LPIR ops to backend `VInst`s.
//!
//! ## Q32 fast-math dispatch
//!
//! `Fadd`/`Fsub`/`Fmul`/`Fdiv` consult `LowerOpts::q32` to choose between
//! the conservative saturating `sym_call`s (default) and inline expansions
//! (Wrapping for add/sub/mul; Reciprocal sym_call for div). The dispatch
//! is in this module and matches `lpvm-wasm`'s `emit/q32.rs` and
//! `emit/ops.rs` bit-for-bit so the browser preview agrees with the
//! device.
//!
//! See `docs/plans-old/2026-04-18-q32-options-dispatch/00-design.md`.
```

### Update existing `lps-builtins` Q32 helper docs

Add a one-line note to each of `fadd_q32.rs`, `fsub_q32.rs`,
`fmul_q32.rs`, `fdiv_q32.rs` near the top:

```rust
//! Reference saturating implementation. Backends inline a faster
//! wrapping/non-saturating expansion when the shader opts into
//! `Q32Options { add_sub: Wrapping, .. }` (resp. `mul: Wrapping` /
//! `div: Reciprocal`). See `__lp_lpir_fdiv_recip_q32` for the reciprocal
//! path.
```

(Adapt wording per file; keep it short.)

### Update `lps-q32` doc

Add to the top of `lps-q32/src/q32_options.rs`:

```rust
//! Per-shader Q32 fast-math options. Wired through `lpir::CompilerConfig`
//! and consumed by `lpvm-native::lower` and `lpvm-wasm::emit`. Defaults
//! to fully-saturating arithmetic.
```

## Step 2: Full workspace validation

Run the full check + test sweep from workspace root:

```bash
turbo check       # or: cargo check --workspace
turbo test        # or: cargo test --workspace
```

Specifically verify:

- `cargo build --workspace --all-targets` — clean build.
- `cargo test --workspace` — all tests pass, no flakes.
- `cargo test -p lps-filetests` (or whatever the runner package is) — all
  filetests pass, including the 4 new ones.
- Build the wasm engine for the browser if there's a separate wasm build
  task (check `turbo.json` or root README) — must build cleanly.

If anything fails, triage:

- **Test flake from the new code:** fix it.
- **Test flake unrelated:** note in `summary.md`, do not block commit.
- **Genuine bug from a phase:** stop and decide whether to fix or revert
  the affected phase.

## Step 3: Cranelift wiring follow-up (if deferred from phase 2)

If phase 2 reported that the cranelift `BuiltinId` registry is generated
and was not updated: either run the generator now (if it's a single
clear command) or document in `summary.md` as a known follow-up:

> Cranelift's `generated_builtin_abi.rs` does not yet know about
> `__lp_lpir_fdiv_recip_q32`. Cranelift codegen does not dispatch on
> `Q32Options` (deprecated path), so this is not blocking; running the
> generator at next sweep will pick it up.

## Step 4: Move plan to `docs/plans-old/`

```bash
mv docs/plans/2026-04-18-q32-options-dispatch \
   docs/plans-old/2026-04-18-q32-options-dispatch
```

(Use the file system rename — the team convention is `docs/plans-old/`,
not `docs/plans-done/`.)

## Step 5: Write `summary.md`

Inside the moved directory, create `summary.md`:

````markdown
# Summary — Q32Options dispatch (2026-04-18)

## What changed

- `lpir::CompilerConfig` gained a `q32: Q32Options` field; both
  `NativeCompileOptions` and `WasmOptions` thread it via `.config`.
- Three new filetest `compile-opt` keys: `q32.add_sub`, `q32.mul`,
  `q32.div`.
- `lps-builtins` gained `__lp_lpir_fdiv_recip_q32` (port of deleted
  `lp-glsl/.../div_recip.rs`, with new `divisor == 0` saturation guard).
- `lpvm-native`: introduced `LowerOpts<'a>`; `lower_lpir_op` dispatches
  `Fadd`/`Fsub`/`Fmul`/`Fdiv` based on `opts.q32`. Wrapping
  `Fadd`/`Fsub` are 1-VInst inlines; wrapping `Fmul` is a 5-VInst
  `mul/mulh/srli/slli/or` sequence; reciprocal `Fdiv` sym_calls the new
  helper.
- `lpvm-wasm`: `EmitCtx` carries `q32`; new `emit_q32_*_wrap` /
  `emit_q32_fdiv_recip` helpers; bit-identical to native by construction.
- `lp-engine` glue: `gfx/native_jit.rs`, `gfx/native_object.rs`,
  `gfx/cranelift.rs` set `config.q32 = options.q32_options` (no longer
  silently dropped).
- 4 new filetests under `scalar/float/q32fast-*.glsl`.

## What did not change

- Defaults: all three modes default to Saturating; existing shaders
  produce identical code.
- `ShaderCompileOptions::q32_options` and cranelift's
  `CompileOptions::q32_options` (top-level fields) remain for API
  stability; they are now consistent with `config.q32`.
- Cranelift codegen is unchanged (still doesn't dispatch on Q32 mode;
  deprecated path).

## Files touched (for grep)

- `lp-shader/lpir/Cargo.toml`
- `lp-shader/lpir/src/compiler_config.rs`
- `lp-shader/lps-q32/src/q32_options.rs`
- `lp-shader/lps-builtins/src/builtins/lpir/fdiv_recip_q32.rs` (new)
- `lp-shader/lps-builtins/src/builtins/lpir/mod.rs`
- `lp-shader/lpvm-native/src/lower.rs`
- `lp-shader/lpvm-native/src/compile.rs`
- `lp-shader/lpvm-wasm/src/emit/mod.rs`
- `lp-shader/lpvm-wasm/src/emit/q32.rs`
- `lp-shader/lpvm-wasm/src/emit/ops.rs`
- `lp-shader/lpvm-cranelift/src/compile_options.rs`
- `lp-core/lp-engine/src/gfx/native_jit.rs`
- `lp-core/lp-engine/src/gfx/native_object.rs`
- `lp-core/lp-engine/src/gfx/cranelift.rs`
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-add-sub.glsl` (new)
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-mul.glsl` (new)
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip.glsl` (new)
- `lp-shader/lps-filetests/filetests/scalar/float/q32fast-div-recip-by-zero.glsl` (new)

## Known follow-ups

- (Document any items punted from phases 2/3/4/5 here, e.g. cranelift
  generator if not run.)

## Validation

- `cargo build --workspace`: clean.
- `cargo test --workspace`: all green.
- `lps-filetests` suite: all green, including 4 new files.
````

(Replace placeholder follow-ups with whatever the phase reports actually
flagged.)

## Step 6: Single commit

Stage all changes from phases 1-6 and commit as one unit:

```bash
git add -A
git status   # eyeball what's staged
git diff --cached --stat   # quick sanity on scope
git commit -m "$(cat <<'EOF'
lpvm: dispatch Q32Options for Fadd/Fsub/Fmul/Fdiv on native + wasm

Wire the existing Q32Options struct through lpir::CompilerConfig into
both lpvm-native and lpvm-wasm so the per-shader fast-math mode actually
controls codegen on the device hot path and the browser preview engine.

- New CompilerConfig::q32 field; filetest compile-opt keys q32.add_sub,
  q32.mul, q32.div.
- New __lp_lpir_fdiv_recip_q32 helper (ported from deleted
  lp-glsl/.../div_recip.rs) with divisor==0 saturation guard.
- lpvm-native: LowerOpts<'a> threading; inline Wrapping AluRRR for
  Fadd/Fsub; 5-VInst mul/mulh/srli/slli/or for Fmul; sym_call to new
  helper for Reciprocal Fdiv.
- lpvm-wasm: EmitCtx carries q32; new emit_q32_{fadd,fsub,fmul}_wrap
  and emit_q32_fdiv_recip; bit-identical to native by construction.
- lp-engine glue stops silently dropping options.q32_options.
- 4 new filetests under scalar/float/q32fast-*.glsl exercise each mode.

See docs/plans-old/2026-04-18-q32-options-dispatch/.
EOF
)"
git status   # confirm clean
```

## Definition of done

- All four `lps-builtins` Q32 helper files have the "reference impl;
  backends inline" doc note.
- `lower.rs` has the dispatch-policy header comment.
- `lps-q32::q32_options` has the wiring header comment.
- `cargo build --workspace` and `cargo test --workspace` are green.
- `summary.md` exists in the moved plan directory and accurately reflects
  what shipped (including any deferrals).
- Plan directory is at `docs/plans-old/2026-04-18-q32-options-dispatch/`.
- Single commit on the current branch covers all of phases 1-6.

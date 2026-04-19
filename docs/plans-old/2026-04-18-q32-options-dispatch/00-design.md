# Q32Options dispatch design

## Scope

Wire `Q32Options` (`lps-q32::q32_options`) through the LPIR → backend
lowering pipeline so per-shader Q32 arithmetic mode actually controls codegen
on **both** `lpvm-native` (device hot path) and `lpvm-wasm` (browser preview
engine).

Both backends must produce **bit-identical** results for the same
`(mode, inputs)` so previewing a shader in the browser shows exactly what
will run on device — including the artifacts of fast-math modes if the user
opts in.

In scope:

- `Q32Options::add_sub` → inline `add`/`sub` (Wrapping) vs sym_call
  saturating helper (Saturating, default).
- `Q32Options::mul`     → inline 32×32→64 mul + shift recombine (Wrapping)
  vs sym_call saturating helper (Saturating, default).
- `Q32Options::div`     → sym_call to new `__lp_lpir_fdiv_recip_q32`
  (Reciprocal) vs sym_call existing saturating helper (Saturating, default).
- New helper `__lp_lpir_fdiv_recip_q32` ported from deleted
  `lp-glsl/.../div_recip.rs` with explicit `divisor == 0` guard.
- `lpir::CompilerConfig` gets a new `q32: Q32Options` field; both
  `NativeCompileOptions::config` and `WasmOptions::config` route through it.
- `compile-opt(q32.add_sub|q32.mul|q32.div, ...)` filetest directives.
- 4 new filetests under `scalar/float/q32fast-*.glsl`.
- `lp-engine` glue stops dropping `q32_options`; sets `config.q32` on both
  native paths.
- Cranelift gets a clarifying TODO; codegen unchanged (deprecated).

Out of scope:

- New algorithms beyond what's already in git history (no Newton-Raphson
  div, no fast-mul saturation tricks, no other `Q32Options` variants).
- Cranelift codegen wiring (deprecated path, vestigial field).
- Filetest runner changes (existing `compile-opt` infra suffices).
- `lp-engine` non-default `glsl_opts` integration tests (covered by future
  roadmap work; v1 is unit + filetest only).

## File structure

```
lp-shader/
├── lpir/
│   ├── Cargo.toml                          # UPDATE: add lps-q32 dep
│   └── src/compiler_config.rs              # UPDATE: q32: Q32Options field;
│                                           #         apply arms for q32.add_sub|
│                                           #         q32.mul|q32.div
│
├── lps-q32/src/q32_options.rs              # UPDATE (if needed): add FromStr
│                                           #         impls for AddSubMode/
│                                           #         MulMode/DivMode (used by
│                                           #         CompilerConfig::apply)
│
├── lps-builtins/src/builtins/lpir/
│   ├── fdiv_recip_q32.rs                   # NEW: port from git (1daa516^);
│   │                                       #      add divisor==0 guard
│   ├── float_misc_q32.rs                   # (unchanged — earlier doc note OK)
│   └── mod.rs                              # UPDATE: register new helper
│
├── lpvm-native/
│   └── src/
│       ├── lower.rs                        # UPDATE: introduce LowerOpts<'a>;
│       │                                   #         dispatch Fadd/Fsub/Fmul/
│       │                                   #         Fdiv on opts.q32; unit
│       │                                   #         tests for both modes
│       └── compile.rs                      # UPDATE: build LowerOpts from
│                                           #         CompileSession.options.config
│
├── lpvm-wasm/
│   └── src/
│       ├── emit/
│       │   ├── mod.rs                      # UPDATE: EmitCtx gets q32 field
│       │   ├── q32.rs                      # UPDATE: + emit_q32_fadd_wrap,
│       │   │                               #         _fsub_wrap, _fmul_wrap,
│       │   │                               #         _fdiv_recip
│       │   └── ops.rs                      # UPDATE: dispatch on ctx.q32
│       └── tests/                          # NEW (or extend existing): unit
│                                           #         tests for both modes
│
├── lpvm-cranelift/
│   └── src/compile_options.rs              # UPDATE: TODO note (vestigial)
│
└── lps-filetests/filetests/scalar/float/
    ├── q32fast-add-sub.glsl                # NEW
    ├── q32fast-mul.glsl                    # NEW
    ├── q32fast-div-recip.glsl              # NEW
    └── q32fast-div-recip-by-zero.glsl      # NEW

lp-core/lp-engine/src/gfx/
├── native_jit.rs                           # UPDATE: stop dropping
│                                           #         options.q32_options;
│                                           #         set config.q32
├── native_object.rs                        # UPDATE: same
└── cranelift.rs                            # UPDATE: also set config.q32
                                            #         (top-level field stays for
                                            #         API stability, both kept
                                            #         consistent)
```

## Conceptual architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│  Producers of Q32Options:                                            │
│  - lp-model::glsl_opts::GlslOpts (per-shader user setting)           │
│  - filetest // compile-opt(q32.{add_sub,mul,div}, ...)               │
└──────────┬───────────────────────────────────────────────────────────┘
           │ map_model_q32_options() / CompilerConfig::apply()
           ▼
┌──────────────────────────────────────────────────────────────────────┐
│  lpir::CompilerConfig                                                │
│    inline: InlineConfig                                              │
│    q32:    Q32Options    ← NEW                                       │
└──────────┬───────────────────────────────────────────────────────────┘
           │ embedded as `.config` in:
           │   NativeCompileOptions { config, ... }
           │   WasmOptions          { config, ... }
           ▼
   ┌───────────────────────────┐         ┌────────────────────────────┐
   │  lpvm-native              │         │  lpvm-wasm                 │
   │                           │         │                            │
   │  CompileSession           │         │  EmitCtx { q32 }           │
   │    options.config.q32     │         │                            │
   │    │                      │         │     │                      │
   │    ▼                      │         │     ▼                      │
   │  LowerOpts<'a> {          │         │  emit/ops.rs match arm:    │
   │    float_mode: FloatMode, │         │    Fadd Q32 ⇒              │
   │    q32: &'a Q32Options    │         │      add_sub == Wrapping ? │
   │  }                        │         │        i32.add :           │
   │    │                      │         │        emit_q32_fadd       │
   │    ▼                      │         │    Fmul Q32 ⇒              │
   │  lower_lpir_op(            │         │      mul == Wrapping ?    │
   │    out, op, opts, ...     │         │        emit_q32_fmul_wrap :│
   │  ) match arm:             │         │        emit_q32_fmul       │
   │    Fadd Q32 ⇒             │         │    Fdiv Q32 ⇒              │
   │      Wrapping ?           │         │      div == Reciprocal ?   │
   │        AluRRR{Add} :      │         │        emit_q32_fdiv_recip:│
   │        sym_call(fadd_q32) │         │        emit_q32_fdiv       │
   │    Fmul Q32 ⇒             │         │                            │
   │      Wrapping ?           │         │                            │
   │        Mul+MulHs+shifts:  │         │                            │
   │        sym_call(fmul_q32) │         │                            │
   │    Fdiv Q32 ⇒             │         │                            │
   │      Reciprocal ?         │         │                            │
   │        sym_call(fdiv_recip):         │                            │
   │        sym_call(fdiv_q32) │         │                            │
   └───────────────────────────┘         └────────────────────────────┘
                  │                                       │
                  └───────────────┬───────────────────────┘
                                  ▼
                ┌────────────────────────────────────┐
                │  lps-builtins (reference impls):   │
                │    __lp_lpir_fadd_q32   (existing) │
                │    __lp_lpir_fsub_q32   (existing) │
                │    __lp_lpir_fmul_q32   (existing) │
                │    __lp_lpir_fdiv_q32   (existing) │
                │    __lp_lpir_fdiv_recip_q32  ← NEW │
                └────────────────────────────────────┘
```

## Dispatch matrix

| Op   | Mode               | lpvm-native                              | lpvm-wasm                          |
| ---- | ------------------ | ---------------------------------------- | ---------------------------------- |
| Fadd | Saturating *(def)* | `sym_call __lp_lpir_fadd_q32`            | `emit_q32_fadd` (i64 widen + sat)  |
| Fadd | Wrapping           | `AluRRR { Add }` (1 VInst)               | `i32.add` (1 op)                   |
| Fsub | Saturating *(def)* | `sym_call __lp_lpir_fsub_q32`            | `emit_q32_fsub`                    |
| Fsub | Wrapping           | `AluRRR { Sub }` (1 VInst)               | `i32.sub`                          |
| Fmul | Saturating *(def)* | `sym_call __lp_lpir_fmul_q32`            | `emit_q32_fmul`                    |
| Fmul | Wrapping           | `mul + mulh + srli + slli + or` (5 VInsts) | i64 widen + mul + shr_s + wrap (6 ops) |
| Fdiv | Saturating *(def)* | `sym_call __lp_lpir_fdiv_q32`            | `emit_q32_fdiv`                    |
| Fdiv | Reciprocal         | `sym_call __lp_lpir_fdiv_recip_q32`      | `emit_q32_fdiv_recip`              |

All Wrapping/Reciprocal expansions are pure deterministic integer math →
bit-identical between backends.

## Algorithm details

### Wrapping `Fmul` (RV32, both backends)

`result = ((a as i64 * b as i64) >> 16) as i32`, wraps mod 2^32.

**RV32 (5 VInsts, 32-bit registers only — no i64 storage):**

```
mul    lo, a, b      ; lo (i32) = bits [31:0]  of a*b
mulh   hi, a, b      ; hi (i32) = bits [63:32] of a*b (signed)
srli   lo, lo, 16    ; bits [31:16] of a*b -> bits [15:0] of lo
slli   hi, hi, 16    ; bits [47:32] of a*b -> bits [31:16] of hi
or     dst, lo, hi   ; dst = bits [47:16] of a*b
```

**Wasm (6 ops):**

```
local.get a; i64.extend_i32_s
local.get b; i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s
i32.wrap_i64
```

Both compute `((a*b) >> 16) & 0xFFFFFFFF` with signed semantics.

### Reciprocal `Fdiv`

Algorithm ported from deleted `lp-glsl/.../div_recip.rs` (commit `1daa516^`):

```rust
fn fdiv_recip_q32(dividend: i32, divisor: i32) -> i32 {
    if divisor == 0 {
        // Match existing __lp_lpir_fdiv_q32 saturation policy.
        return if dividend == 0 { 0 }
               else if dividend > 0 { MAX_FIXED }
               else { MIN_FIXED };
    }
    let result_sign = if (dividend ^ divisor) < 0 { -1 } else { 1 };
    let recip = 0x8000_0000u32 / (divisor.unsigned_abs());
    let q = (((dividend.unsigned_abs() as u64) * (recip as u64) * 2u64) >> 16) as u32;
    (q as i32) * result_sign
}
```

- 1 i32 udiv + 2 muls + shift + sign fixup.
- Native: `sym_call` to new helper. (No inline expansion in v1 — the
  algorithm has enough sequencing that an inline would be much larger than
  a call site.)
- Wasm: inlined as `emit_q32_fdiv_recip` to match native's externally-
  observable semantic bit-for-bit (uses i32 div_s + i64 mul + shift +
  signed fixup).

Documented error: ~0.01% typical, ~2-3% at edges (saturated dividends, very
small divisors).

## Test strategy

1. **Unit tests in `lower.rs`** — assert VInst sequences for both modes of
   `Fadd`, `Fsub`, `Fmul`, `Fdiv`. Saturating (default) tests already exist
   and remain unchanged.
2. **Unit tests in `lpvm-wasm`** — assert wasm output bytes (or use
   wasmtime to execute small fragments) for both modes of each op.
3. **Cross-backend semantic guard** — small property-style test (in
   `lps-builtins` or a shared crate) that for representative `(mode, a, b)`
   inputs runs:
   - the lps-builtins reference helper (Saturating)
   - a Rust port of the wrapping/reciprocal expansion
   asserts equal i32 outputs across `(Saturating, default helper)` and
   `(Wrapping/Reciprocal, expansion)` pairs. Documents the algorithm in code
   form too.
4. **Filetests** — 4 new files exercising each fast-math mode end-to-end
   through GLSL → LPIR → backend → execution on `rv32n.q32` (and wasm where
   the runner supports it). See `00-notes.md` Q8.

## Backwards compatibility

- `ShaderCompileOptions::q32_options` (lp-engine top-level) stays — the
  engine API doesn't change shape.
- `CompileOptions::q32_options` (cranelift top-level) stays — TODO comment
  notes it's vestigial; codegen is unchanged.
- Default for all three modes is `Saturating` / `Saturating` / `Saturating`
  → existing shaders compile to identical code.
- New `q32` field on `CompilerConfig` defaults to `Q32Options::default()`
  → no change for any caller that doesn't opt in.

## Plan phases

1. **Foundation: `CompilerConfig` promotion + engine glue**
   `[sub-agent: yes, parallel: -]`
   Add `lpir → lps-q32` dep. Add `q32: Q32Options` to `CompilerConfig`.
   Add `apply` arms. Update lp-engine glue to stop dropping `q32_options`.
2. **`fdiv_recip_q32` helper (port from git)**
   `[sub-agent: yes, parallel: 4]`
   New file in `lps-builtins`. Port algorithm + tests from `1daa516^`.
   Add `divisor == 0` guard. Register in builtins module.
3. **`lpvm-native` dispatch**
   `[sub-agent: yes, parallel: -, depends: 1, 2]`
   Introduce `LowerOpts<'a>`. Refactor `lower_lpir_op` signature. Implement
   Wrapping inlines for Fadd/Fsub/Fmul. Implement Reciprocal sym_call for
   Fdiv. Unit tests for both modes of all four ops.
4. **`lpvm-wasm` dispatch**
   `[sub-agent: yes, parallel: 2]`
   Add `q32: Q32Options` to `EmitCtx`. New `emit_q32_*_wrap` /
   `emit_q32_fdiv_recip` helpers. Update `emit/ops.rs` dispatch. Unit tests.
5. **Filetests**
   `[sub-agent: yes, parallel: -, depends: 3, 4]`
   4 new files in `scalar/float/q32fast-*.glsl`. Validate by running
   filetest suite.
6. **Cleanup, validate, summary, commit**
   `[sub-agent: supervised]`
   Update `lower.rs` policy doc. Update existing helper docs. Run full
   `turbo check test`. Write `summary.md`. Move plan to `docs/plans-old/`.
   Single commit at end.

Phase 1 must complete before any other phase starts (provides
`CompilerConfig::q32` that 3 and 4 read from).

Phase 2 must complete before phase 3 (provides the `__lp_lpir_fdiv_recip_q32`
symbol that phase 3's sym_call references at link time).

Phases 3 and 4 touch disjoint crates; once phase 2 is done they can run in
parallel.

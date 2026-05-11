# Q32Options dispatch through the lowering pipeline

Plan dir: `docs/plans/2026-04-18-q32-options-dispatch/`
Started: 2026-04-18

## Scope of work

Wire `Q32Options` (`lps-q32::q32_options`) through the LPIR → backend lowering
pipeline so per-shader Q32 arithmetic mode actually controls codegen.

The model and engine layer already plumb `glsl_opts` → `Q32Options` to the
backend boundary, but **no backend currently reads it**. All three backends
(`lpvm-native`, `lpvm-cranelift`, `lpvm-wasm`) unconditionally emit the
saturating helper for Q32 `Fadd` / `Fsub` / `Fmul`.

The immediate motivation is performance: in `lpvm-native` the saturating helper
is a `sym_call` with full caller-save overhead, on the per-pixel hot path. With
`AddSubMode::Wrapping` we can inline a single `add` / `sub` VInst.

### In scope (v1)

Both **`lpvm-native`** (device hot path) and **`lpvm-wasm`** (preview engine)
must dispatch on `Q32Options` so preview matches device. Semantics on both
backends must be bit-identical for the same `(mode, inputs)`.

1. **`lpvm-native`**: thread `Q32Options` into `lower_lpir_op` and emit:
   - `Fadd` / `Fsub`:
     - `Saturating` (default) → existing `sym_call` to `__lp_lpir_f{add,sub}_q32`.
     - `Wrapping` → inline `VInst::AluRRR { Add | Sub }` (1 VInst).
   - `Fmul`:
     - `Saturating` (default) → existing `sym_call` to `__lp_lpir_fmul_q32`.
     - `Wrapping` → inline Q32 mul (`mul + mulh + shift-pair recombine`,
       ~4 VInsts). See Q3 below.
2. **`lpvm-wasm`**: thread `Q32Options` into the wasm emit ctx and emit:
   - `Fadd` / `Fsub`:
     - `Saturating` (default) → existing `emit_q32_fadd / _fsub` (i64 widen +
       sat).
     - `Wrapping` → `i32.add` / `i32.sub` (single wasm op, wraps modulo 2^32 —
       matches RV32 `add`/`sub` exactly).
   - `Fmul`:
     - `Saturating` (default) → existing `emit_q32_fmul`.
     - `Wrapping` → `i64.extend_i32_s` × 2 + `i64.mul` + `i64.const 16` +
       `i64.shr_s` + `i32.wrap_i64` (wasm equivalent of the RV32
       `mul+mulh+recombine`). Bit-identical to native wrapping output.
3. Plumbing:
   - `NativeCompileOptions` → `CompileSession` → `lower_ops` → `lower_lpir_op`.
   - Wasm equivalent: `WasmCompileOptions` (or whatever the analogous struct is
     — to be confirmed in the wasm-side audit, see Q5) → emit ctx →
     `emit_q32_*`.
4. **`lp-engine` glue** stops dropping `q32_options`:
   - `gfx/native_jit.rs`: pass into `NativeCompileOptions`.
   - `gfx/wasm.rs` (or equivalent): pass into wasm options.
   - `gfx/native_object.rs`: pass into `NativeCompileOptions`.
5. **Tests**:
   - `lpvm-native`: unit tests in `lower.rs` covering both modes for
     `Fadd` / `Fsub` / `Fmul`.
   - `lpvm-wasm`: unit tests covering both modes for `Fadd` / `Fsub` / `Fmul`.
   - **Cross-backend semantic guard**: a small set of property-style tests
     that drive the same `(a, b)` inputs through both backends in both modes
     and assert bit-identical i32 results. This is the contract that justifies
     "preview matches device". See Q4 below.
6. Update the inlining-policy doc comment at top of `lpvm-native`'s `lower.rs`
   to reflect the new `Q32Options::{add_sub, mul}` behaviour.

### Out of scope (v1, deferred)

- **`Fdiv` / `DivMode::Reciprocal`**: requires reciprocal multiply or shift-by-
  constant tricks; large enough to deserve its own plan. Both backends keep
  current saturating helper.
- **`lpvm-cranelift`**: vestigial field stays vestigial. Cranelift is the
  deprecated host JIT path; we're not adding new code paths to it. Will note
  in `lpvm-cranelift/src/compile_options.rs` that the field is wired by
  the engine but not consumed by codegen. (Q1 confirms.)
- **Filetest annotation for `q32_options`**: no infrastructure today; would
  require extending `lps-filetests` parser. Q4 below.
- **Saturation as a backend-toggleable inline expansion**: the saturating
  helper is correct enough, and an inline saturating expansion is ~7 VInsts;
  not in scope.
- **A "naked imul" mode**: only the proper Q32 wrapping mul is exposed. See
  Q3.

## Current state of the codebase

### `Q32Options` definition

`lp-shader/lps-q32/src/q32_options.rs` defines:

```rust
pub struct Q32Options { pub add_sub: AddSubMode, pub mul: MulMode, pub div: DivMode }
pub enum AddSubMode { Saturating /* default */, Wrapping }
pub enum MulMode    { Saturating /* default */, Wrapping }
pub enum DivMode    { Saturating /* default */, Reciprocal }
```

### Engine-level mapping is already done

`lp-core/lp-engine/src/nodes/shader/runtime.rs:28` already maps
`lp_model::glsl_opts::GlslOpts` → `lps_q32::q32_options::Q32Options` and packs
it into `ShaderCompileOptions { q32_options, max_errors }`
(`lp-engine/src/gfx/lp_shader.rs`). This reaches each backend's
`compile_shader(...)`.

### Where `q32_options` is currently dropped

| Backend          | Site                                                                       | Today                                                       |
| ---------------- | -------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `lpvm-native`    | `lp-engine/src/gfx/native_jit.rs:89` — `let _ = (... options.q32_options)` | Explicitly dropped. `NativeCompileOptions` has no field.    |
| `lpvm-cranelift` | `lp-engine/src/gfx/cranelift.rs:49` — `q32_options: options.q32_options`   | Stored on `CompileOptions` but never read in `emit/scalar`. |
| `lpvm-wasm`      | (similar — wasm backend has no `q32_options` field at all)                 | Not stored, not read.                                       |

### Current `lpvm-native` lowering

`lp-shader/lpvm-native/src/lower.rs` (post the 2026-04-18 inlining refactor):

- `lower_lpir_op` signature already takes `out: &mut Vec<VInst>`,
  `temps: &mut TempVRegs`, threads everything from `LowerCtx`.
- Q32 `Fadd` / `Fsub` / `Fmul` / `Fdiv` arms all call `sym_call(...)` to the
  saturating helper.
- `LowerCtx` (line 1130) holds `float_mode: FloatMode` but **not**
  `q32_options`.
- `lower_ops(func, ir, abi, float_mode)` (line 1579) is the public entry; called
  from `compile_function` (compile.rs:110) which has `session: &mut
  CompileSession` containing `options: NativeCompileOptions`.

### Reference implementations

- `lps-builtins/src/builtins/lpir/fadd_q32.rs`, `fsub_q32.rs`,
  `fmul_q32.rs`: i64 widen + saturate (clamp to `MIN_FIXED..=MAX_FIXED`).
- These remain authoritative for the `Saturating` path.

### Current `lpvm-wasm` lowering

- `lp-shader/lpvm-wasm/src/options.rs::WasmOptions` has only `{ float_mode,
  config }`. **No `q32_options` field.**
- `lp-shader/lpvm-wasm/src/emit/q32.rs` defines `emit_q32_fadd / _fsub /
  _fmul / _fdiv / _fabs / _itof_*` — all unconditional, all i64-widen-and-
  saturate for add/sub/mul.
- `lpvm-wasm/src/emit/ops.rs:491-532` calls these directly inside a `match fm`
  on `FloatMode`. No options branching.
- `EmitCtx` (`emit/mod.rs:25`) holds `options: &WasmOptions`, so threading
  `options.q32_options` down to `emit_q32_*` callers is a one-field hop.
- **Consumers of `WasmOptions`** (where engine-level glue would set
  `q32_options`):
  - `lp-app/web-demo/src/lib.rs:28` — browser preview (the user's "preview
    engine"). Sets it directly today; needs to pull from a JS-passed user
    setting.
  - `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs:197` — filetest
    runner.
  - `lp-shader/lp-shader/src/tests.rs:11` — host wasmtime tests.
  - `lp-shader/lpvm-wasm/tests/runtime_shared_memory.rs` — wasm runtime tests.
- **`lp-engine` does not consume `lpvm-wasm`** today — wasm preview lives in
  the web-demo (and any future authoring tool), not in `lp-engine`. So the
  engine-layer plumbing for `q32_options` only needs to happen for
  `lpvm-native`. The wasm-side glue is the web-demo.

### Other backends (for awareness)

- `lp-shader/lpvm-cranelift/src/emit/scalar.rs:25-62`: Q32 arms unconditionally
  call `refs.fadd / .fsub / .fmul`. `EmitCtx` has `float_mode` but no
  `q32_options`. Cranelift JIT is deprecated → out of scope (see Q1).

## Questions

### Q1 — Scope: which backends?

*(Answered 2026-04-18: native + wasm; cranelift deferred.)*

Both `lpvm-native` (device) and `lpvm-wasm` (preview engine) must dispatch on
`Q32Options` so preview matches device exactly. Without that, a user opting
in to fast math sees one set of artifacts in the browser and a different set
on the device — the whole point of preview is broken.

`lpvm-cranelift` is deprecated and stays vestigial (with a clarifying comment
on the unused field).

### Q2 — Threading shape

**Answered 2026-04-18:**

- **Native: B by reference.** New `LowerOpts<'a> { float_mode: FloatMode,
  q32: &'a Q32Options }` (or similar). `lower_lpir_op` takes
  `opts: &LowerOpts<'_>`. `LowerCtx` holds `LowerOpts` directly.
  By-ref because passing by value gains nothing (struct is small but the
  call sites are tight loops; ref is at least as cheap and sets the pattern
  for future fields like `IsaTarget`).
- **Wasm: A.** Just add `q32: Q32Options` to `EmitCtx` (already a struct
  threaded everywhere). No new wrapper.

**Context:** Two reasonable shapes:

A. Add `q32_options: Q32Options` to `NativeCompileOptions`, then to
   `CompileSession`, then add a `q32_options: Q32Options` field to `LowerCtx`,
   then add a `q32_options: Q32Options` parameter to `lower_lpir_op` (or
   bundle it into a small `LowerOpts` struct alongside `float_mode`).

B. Same as A but bundle `(float_mode, q32_options)` into a single
   `LowerOpts` struct passed to `lower_lpir_op`. Keeps the signature stable
   for future options.

**Suggested:** **B.** `lower_lpir_op` already has 10 parameters. A
`LowerOpts { float_mode, q32_options }` struct read-only by-value is cheaper to
extend later (e.g. `IsaTarget` for Zbb, future fast-math flags) and trims the
signature. Cost: one mechanical refactor of every call site (test helpers +
`LowerCtx`).

### Q3 — `Fmul` wrapping semantic

**Context:** Q32 multiply in fixed-point isn't simply `imul`. The proper
unsaturated wrapping result is `((a as i64 * b as i64) >> 16) as i32` — so two
RV32 instructions (`mul` + `mulh`, then a fused 64→32 normalisation: `srli /
slli / or` or shift-pair ~3 insts), or roughly **5 VInsts** for the
shift-pair lowering. Pure `imul` (truncated 32-bit mul, no shift) is a single
instruction but is the wrong number — it scales the result by `1/65536`.

If we treat `MulMode::Wrapping` as "wrap, but still semantically a Q32
multiply", the inline expansion is ~5 VInsts. Comparable to a `sym_call`
in code size (call overhead is ~6-7 VInsts per call site under fastalloc:
caller-save spills, jal, restore), but avoids spilling live regs across the
call.

If we treat `MulMode::Wrapping` as "give me raw 32-bit `imul`", that's 1 VInst
but probably breaks every shader.

**Answered 2026-04-18:** Lock in the **proper Q32 >>16 wrapping mul**.

**Native (RV32IMAC) — 5 VInsts, all 32-bit registers:**

```
mul    lo, a, b      ; lo (32-bit) = bits [31:0]  of a*b
mulh   hi, a, b      ; hi (32-bit) = bits [63:32] of a*b   (signed)
srli   lo, lo, 16    ; bits [31:16] of a*b -> bits [15:0] of lo
slli   hi, hi, 16    ; bits [47:32] of a*b -> bits [31:16] of hi
or     dst, lo, hi   ; dst = bits [47:16] of a*b
```

(RV32 has no 64-bit values; `mul`/`mulh` is how the M-extension exposes
"give me both 32-bit halves of the 32×32→64 product".)

**Wasm — 6 ops, bit-identical to native:**

```
local.get a; i64.extend_i32_s
local.get b; i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s
i32.wrap_i64
```

Both compute `((a*b) >> 16) & 0xFFFFFFFF` with signed semantics; outputs
match bit-for-bit for all `(a, b)`.

**No "naked imul" mode.** Off by 1/65536 vs the proper Q32 mul; never useful
for a shader.

### Q4 — Filetest coverage

**Updated 2026-04-18 — infra already exists.**

`lps-filetests` already has a `compile-opt(key, value)` directive (see
`lp-shader/lps-filetests/src/parse/parse_compile_opt.rs`) that builds a
`lpir::CompilerConfig` via `CompilerConfig::apply(key, value)`
(`lp-shader/lpir/src/compiler_config.rs:108`). `CompilerConfig` is then
threaded into both `NativeCompileOptions::config` and `WasmOptions::config`,
so any new key reaches both backends with no runner changes.

Current keys: `inline.mode`, `inline.always_inline_single_site`,
`inline.small_func_threshold`, `inline.max_growth_budget`,
`inline.module_op_budget`. Pattern: dotted namespace, validated values.

**Plan:**

1. **Promote `q32_options` into `CompilerConfig`.** Add a
   `pub q32: lps_q32::Q32Options` field (or analogous), so it lives next to
   `inline` and gets the same plumbing for free. This becomes the
   single source of truth for Q32 arithmetic mode.
   - Existing `NativeCompileOptions::config` and `WasmOptions::config`
     fields don't change shape.
   - `lp-engine`'s `gfx/native_jit.rs` etc. set
     `options.config.q32 = engine_q32_options` instead of (or in addition
     to — see Q2) a top-level `q32_options` field.
2. **Add `apply` arms** for:
   - `q32.add_sub` → values `saturating` | `wrapping`
   - `q32.mul`     → values `saturating` | `wrapping`
   - `q32.div`     → values `saturating` | `reciprocal`
3. **New test files** — see Q8 for placement.

This eliminates Q5's suggestion to add a `q32_options` field to
`NativeCompileOptions` / `WasmOptions` directly: the field already lives in
`config`, both options structs already carry `config`, both backends already
read `options.config.*`. Simpler.

(One caveat: `lpir` is a no-std crate, currently has no dependency on
`lps-q32`. Adding the dep is straightforward — both crates are inside the
`lp-shader` workspace and `lps-q32` is no-std-friendly. Worth confirming —
see Q9 below.)

### Q5 — Where to set Q32Options for native_jit / native_object / web-demo?

**Context:** The `lp-engine` glue currently constructs `NativeCompileOptions`
with explicit field overrides and `..Default::default()`. Once the field
exists, the glue just sets `q32_options: options.q32_options`.

**Open:** is there any non-engine consumer of `NativeCompileOptions` that
should care? Searched — only `lp-cli/src/commands/shader_debug` uses
`NativeCompileOptions` outside `lpvm-native` itself, and it can take the
default.

**Updated 2026-04-18 — superseded by Q4.** Since `q32_options` lives in
`CompilerConfig` (which `NativeCompileOptions` and `WasmOptions` already
embed via their `config` fields), there's no separate "where to set it" hop.
Engine layer sets `options.config.q32 = engine_q32_options`. Web-demo same.
Filetest runner sets it via `CompilerConfig::apply("q32.add_sub", "wrapping")`.
Tests stay on defaults unless they opt in.

### Q6 — Default behaviour & test policy

**Context:** Default is `AddSubMode::Saturating`, so existing behaviour is
preserved verbatim. New unit tests should cover:

- `Fadd` Saturating → unchanged `sym_call` (existing test stays as-is).
- `Fadd` Wrapping → 1 `VInst::AluRRR { Add }`.
- `Fsub` Saturating → unchanged.
- `Fsub` Wrapping → 1 `VInst::AluRRR { Sub }`.
- `Fmul` Saturating → unchanged.
- `Fmul` Wrapping → ~4 VInsts matching the proper Q32-wrapping mul shape.

`Fdiv` stays `sym_call` regardless of `DivMode` in v1.

**Suggested:** As above. Plus: assert that `Q32Options::default()` produces
identical VInst sequences to the pre-refactor lowering for at least one
representative case (regression guard).

### Q7 — Should v1 also include `DivMode::Reciprocal`?

**Updated 2026-04-18 — algorithm exists in git history.**

The reference implementation lives in deleted file
`lp-glsl/crates/lp-glsl-compiler/src/backend/transform/fixed32/reference/div_recip.rs`
(deleted in commit `1daa516`, "refactor: finish renaming Fixed32 -> Q32"),
and there's accompanying research notes in
`docs/plans/2026-02-09-glsl-fast-math-mode/research-recip-mul.md`
(deleted in `5f0d4b4d`).

**Algorithm (signed):**

```
recip = 0x8000_0000 / divisor.unsigned_abs()    // one i32 udiv, truncates
quot  = ((|dividend| as u64 * recip as u64 * 2) >> 16) as u32
quot *= sign(dividend) ^ sign(divisor)
```

Trades 1 expensive `div` for 1 cheap `udiv` (precomputable when divisor is
constant!) + 2 `mul` + 1 shift. Documented error: **~0.01% typical, up to
~2-3% at edges** (saturated values, very small divisors).

**Implementation effort for v1:**

| Piece                | Status |
| -------------------- | ------ |
| Algorithm + tests    | Port from `div_recip.rs` (~50 lines, well-documented) |
| Native helper        | New `lps-builtins/.../fdiv_recip_q32.rs` |
| Native dispatch arm  | `DivMode::Reciprocal` → `sym_call("__lp_lpir_fdiv_recip_q32")` |
| Wasm helper          | New `emit_q32_fdiv_recip` — i64 mul + shift recombine, ~15 wasm ops |
| Wasm dispatch arm    | One match arm in `emit/ops.rs` |
| Cross-backend guard  | Already in v1 plan; just add `(DivMode, dividend, divisor)` cases |
| Div-by-zero handling | Mirror the existing `__lp_lpir_fdiv_q32` saturation policy |

Estimated **+25-30% over the add/sub/mul-only scope**, not the doubling I
originally feared. Both backends use deterministic integer math so
bit-identical preview is enforceable.

**Suggested:** **Include `Fdiv` in v1.** Same dispatch infrastructure work,
straightforward port, well-tested algorithm, real perf win on embedded
(`div` is the slowest M-extension instruction). Saves a follow-up plan that
would mostly repeat the same plumbing.

If review surface grows uncomfortable mid-implementation, the natural fall-
back is to land `Fdiv` as a separate phase in the same plan — keeps the
dispatch infrastructure together, splits the algorithm-port commit if
needed.

**Sub-question (Q7a) — answered 2026-04-18:** Div-by-zero in the reciprocal
path: **add the explicit branch**. Cannot trap (wasm `i32.div_s` traps on
zero, native would trap on `0x8000_0000 / 0`). The new helper guards
`divisor == 0` and saturates to `MIN_FIXED` / `MAX_FIXED` / `0` matching the
existing `__lp_lpir_fdiv_q32` semantic. Tiny cost vs the 1 udiv saved.

### Q8 — Where do new fast-math filetests live?

**Context:** Existing categories under `lp-shader/lps-filetests/filetests/`:
`array, builtins, const, control, debug, function, global, global-future,
lpfn, lpvm, matrix, operators, scalar, struct, type_errors, uniform, uvec*,
vec, vmcontext`.

Existing pattern for Q32-specific tests already established:
`scalar/float/q32-div-by-zero.glsl` lives next to its `op-divide.glsl`
counterpart and uses `// @ignore(float_mode=f32)` to skip on f32.

**Options:**

A. **`scalar/float/` with prefix**, e.g.:
   - `scalar/float/q32-fast-add-sub.glsl` (`compile-opt(q32.add_sub, wrapping)`)
   - `scalar/float/q32-fast-mul.glsl` (`compile-opt(q32.mul, wrapping)`)
   - `scalar/float/q32-fast-div-recip.glsl` (`compile-opt(q32.div, reciprocal)`)
   - `scalar/float/q32-fast-div-recip-by-zero.glsl` (Q7a coverage)

B. **New top-level `fast-math/` directory** dedicated to per-shader code-gen
   options. Currently empty; would host these and any future fast-math tests.

C. **Per-op spread**: `q32-fast-add.glsl`, `q32-fast-sub.glsl`,
   `q32-fast-mul.glsl`, `q32-fast-div-recip.glsl` (one per op).

**Answered 2026-04-18:** **A with `q32fast-` prefix** (one tag, less hyphen
noise than `q32-fast-`). Concrete file list:

- `scalar/float/q32fast-add-sub.glsl` (`compile-opt(q32.add_sub, wrapping)`)
- `scalar/float/q32fast-mul.glsl` (`compile-opt(q32.mul, wrapping)`)
- `scalar/float/q32fast-div-recip.glsl` (`compile-opt(q32.div, reciprocal)`)
- `scalar/float/q32fast-div-recip-by-zero.glsl` (Q7a saturation coverage)

All gated `// @ignore(float_mode=f32)`. `add` and `sub` share one file —
they're symmetric and share the `q32.add_sub` knob. If volume grows beyond
~6-8 files, promote to `fast-math/`.

### Q9 — `lpir` ↔ `lps-q32` dependency direction

**Context:** Q4 proposes adding `Q32Options` (defined in `lps-q32`) as a
field on `lpir::CompilerConfig` (defined in `lpir`). Today `lpir` has no
dep on `lps-q32`.

Quick look:
- `lp-shader/lps-q32/Cargo.toml` — to be confirmed: needs to be no-std,
  must not depend on `lpir` (would create a cycle).
- `lp-shader/lpir/Cargo.toml` — currently no `lps-q32` dep.

**Options:**

A. Add `lpir` → `lps-q32` dependency. `Q32Options` lives in `lps-q32`,
   referenced from `CompilerConfig`. Clean, single source of truth.
   Requires `lps-q32` to have no dep on `lpir`.

B. Move `Q32Options` from `lps-q32` into `lpir` (or up to a shared crate).
   Removes the dep direction question but is a rename touching
   `lp-engine`, `lpvm-cranelift`, `lpvm-native`, `lpvm-wasm`, etc.

C. Mirror the type: define a parallel `lpir::Q32Mode` and have engine code
   convert. Avoids the dep but creates duplicate types — exactly the
   anti-pattern that `lps-q32` was extracted to fix.

**Suggested:** **A.** `lps-q32` is small, focused, and already no-std
(needed by emu paths). Adding it as a dep of `lpir` is the right shape:
`lpir` consumes Q32 mode flags but doesn't define them.

**Verified 2026-04-18:** Both `lpir` and `lps-q32` have zero workspace deps
(only `libm`). Adding `lpir → lps-q32` is a one-line `Cargo.toml` change
with no cycle risk and no no-std implications.

# Notes

## 2026-04-18 — Q1 answered

Both `lpvm-native` and `lpvm-wasm` must dispatch on `Q32Options` so preview
matches device. Cranelift JIT is deprecated and stays vestigial.

## 2026-04-18 — Q4 / Q5 answered

Promote `q32_options` into `lpir::CompilerConfig` as a `q32` field. Single
source of truth. Filetests get `compile-opt(q32.add_sub | q32.mul | q32.div,
...)` for free via existing `CompilerConfig::apply` infrastructure. Engine
glue sets `options.config.q32 = ...` instead of a separate
`q32_options` field on `NativeCompileOptions` / `WasmOptions`.

## 2026-04-18 — Q7 / Q7a answered

`Fdiv` (`DivMode::Reciprocal`) **is in scope for v1** — algorithm exists in
git (`lp-glsl/.../div_recip.rs`, deleted in `1daa516`); port + dispatch is
~+25-30% over add/sub/mul. Helper guards `divisor == 0` explicitly to match
the existing `__lp_lpir_fdiv_q32` saturation policy and avoid trapping on
either backend.

## 2026-04-18 — Q8 answered

Filetests live in `scalar/float/q32fast-*.glsl`. See Q8 for the file list.

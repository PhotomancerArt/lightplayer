# LPIR middle-end optimization opportunities

## Status: deferred (post-release)

Notes from a perf exploration on 2026-04-19. Real product features come
first; nothing in here changes correctness or unblocks shipping. Captured
so the thinking isn't lost.

## Context

Latest CPU profile (`profiles/2026-04-19T22-56-15--examples-basic--steady-render--wrapping-reciprocal/`)
shows the usual suspects for a JIT'd shader on ESP32-C6:

- `fixture::render` self time (~11%)
- `__lp_lpir_fdiv_recip_q32` self ~9%, inclusive ~9.4%
- JIT'd shader code blocks (the big inclusive numbers)
- `__lp_lpir_ffloor_q32`, `__lp_lpfn_psrdnoise2_q32`, `__lps_sin_q32`
- `__divdi3` self ~1.9% (saturating Q32 divide path)

Two distinct optimization stories live in this profile. Both need the
inliner (currently on `feature/inline` branch, not merged due to code size) before they
really pay off.

## Small story: inline `__lp_lpir_fdiv_recip_q32` in lpvm-native

Today `DivMode::Reciprocal` lowers to a single `sym_call` in
[`lp-shader/lpvm-native/src/lower.rs`](../../lp-shader/lpvm-native/src/lower.rs)
(see the `LpirOp::Fdiv` arm around the `BuiltinId::LpLpirFdivRecipQ32`
selection). The wasm backend already inlines the same algorithm in
[`emit_q32_fdiv_recip`](../../lp-shader/lpvm-wasm/src/emit/q32.rs).

Effort to inline on native:

- Add `AluOp::MulHu` (RV32M `mulhu`, funct3=011, funct7=1) — `MulH`
  alone is _signed_ and is not interchangeable for the reciprocal
  algorithm's `(abs_dividend as u64) * (recip as u64)` step.
- Lower `Fdiv` (Reciprocal mode) to ~15-30 VInsts mirroring
  `emit_q32_fdiv_recip`: div0 saturation tree, abs, `divu` for the
  reciprocal, `mul`+`mulhu` wide product, shift, sign apply.
- Bit-exact tests against the helper (the helper stays as the gold
  reference, same pattern as wrapping `Fmul`).

Magnitude of win: removes a call/save/restore + ABI marshalling around
a ~50-cycle inner core. Probably 2-4% of total, not transformative.

### Algorithm itself: not much fruit there

Surveyed the literature (Jones reciprocal-multiplication notes,
libfixmath `fix16_div`, Newton-Raphson papers). For arbitrary runtime
divisors on a CPU with hardware `divu` and `mul`/`mulh` but no `clz`
(ESP32-C6 doesn't enable Zbb), the current "1 `divu` + 1 wide mul"
pattern is essentially the textbook fast path. Newton-Raphson would
trade `divu` for ~2 mul-iterations + LUT + emulated CLZ — likely a
wash on this core, possibly worse. libfixmath's path is more accurate
(bit-exact) but slower. So if we want a faster divide, the win is in
_avoiding_ the divide (LICM, hoist invariant reciprocals, magic-number
divides for compile-time constants), not in changing the algorithm.

### Bigger lever: const-divisor specialization

Higher-leverage than inlining the helper, and lands without needing
the inliner. When the divisor is a compile-time constant, the runtime
`divu` evaporates entirely:

1. **Trivial path (Reciprocal mode)**: rewrite `Fdiv(x, c)` to a
   multiply by a precomputed reciprocal:

   ```rust
   let recip2: u32 = (0x8000_0000u32 / abs_divisor) * 2;  // compile-time
   // runtime: (dividend as u64 * recip2 as u64) >> 16, sign-applied
   ```

   Bit-identical to `__lp_lpir_fdiv_recip_q32` on the same input, but
   no `divu` and no call — runtime drops from ~50 cycles to ~10
   (`mul`+`mulhu`+shift+sign). Lives as a match arm in
   [`const_fold.rs`](../../lp-shader/lpir/src/const_fold.rs); the
   per-vreg `vreg_val: Vec<Option<i32>>` infrastructure is already
   there. Maybe 50-100 LOC.

2. **Power-of-2 special case**: `Fdiv(x, 2^k)` → arithmetic shift right
   (with sign care). Subset of (1).

3. **Saturating-mode bit-exact**: Granlund-Montgomery "magic number"
   divide (`mulh` + shift + add, no `divu`). What every modern C
   compiler does for `n / const`. More work than (1) and only needed
   if we want bit-exactness for const-divisor cases in the saturating
   path.

This composes with LICM: even when the divisor is a uniform rather
than a literal, LICM hoists the reciprocal computation out of the
per-pixel loop, so per-iteration cost is still one multiply. Same end
result, broader applicability. (1) plus a working LICM could plausibly
drop `__lp_lpir_fdiv_recip_q32` out of the top-20 entirely.

Pre-flight before doing this: grep the example shaders for how many
`Fdiv` ops have literal divisors today. If it's ~zero, skip straight
to LICM and revisit.

## Big story: LPIR middle-end passes

Once the inliner merges, `render()` and its helpers collapse into one
function with the per-pixel loop visible. That unlocks:

1. **Loop-Invariant Code Motion (LICM)** — biggest expected win.
   Anything depending only on uniforms / vmctx fields gets recomputed
   per-pixel today; should be hoisted to the loop preheader.
2. **Common Subexpression Elimination (CSE)** — inlining merges
   helper-internal expressions with caller expressions, exposing matches.
3. **Stack-slot store-to-load forwarding** — track last `store` to a
   `slot_addr`, fold subsequent `load` to use the stored value. Often
   dominates everything else when `RenderContext`-style code is involved.
4. **Strength reduction / IV analysis** — turn `i*stride` into another
   induction variable that adds `stride` per iteration. Big for sampler
   address math.
5. **Extended constant folding** — `lpir/src/const_fold.rs` already does
   the local version. Post-inlining, callee constants should propagate
   into caller branches.
6. **Algebraic peephole + DCE** — small per-pass wins, large in aggregate.

### Feasibility under non-SSA LPIR

LPIR is non-SSA by [explicit design decision](../design/lpir/00-overview.md)
(decision #6). Reassignment is allowed; targets rebuild SSA as needed.
Implication for middle-end passes:

| Pass                             | Without-SSA difficulty                                                                                                                          |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Algebraic peephole               | Trivial                                                                                                                                         |
| Local CSE within an EBB          | Easy (hash on `(op, operand vregs)`, invalidate on reassignment; EBB ends at any control-flow marker or impure call)                            |
| Extended const fold              | Easy (same shape as `const_fold.rs`)                                                                                                            |
| Local DCE                        | Easy (use-counting)                                                                                                                             |
| Slot store-to-load forwarding    | Easy-medium (per-region last-write tracking)                                                                                                    |
| **LICM**                         | **Medium** — needs per-loop "modified vregs" set + speculation safety check. Doable; structured `LoopStart`/`Continue` makes loop nesting free. |
| Global CSE                       | Medium-hard — vreg name ≠ value across reassignment. Want value numbering keyed on definition site.                                             |
| IV analysis / strength reduction | Hard without SSA — bookkeeping starts to look like SSA-lite.                                                                                    |
| SCCP / GVN-PRE                   | Just build SSA at this point.                                                                                                                   |

LPIR has structural advantages for these passes:

- Structured control flow: loop nesting is free, no CFG to derive.
- Scalarized + ANF: each op is already a single named computation.
- Dense `vreg_count`: per-pass `Vec<bool>` keyed by vreg index is cheap.
- **All scalar arithmetic is non-trapping** (per
  [00-overview.md numeric semantics](../design/lpir/00-overview.md);
  Q32 mode also non-trapping by construction). This is huge for LICM
  speculation safety: pure scalar ops can be hoisted unconditionally.

The annoying parts are **loads** (could read garbage if hoisted past a
guard — be conservative) and **calls** (need pure-call attribution on
import declarations; worth adding regardless).

## Suggested phasing if/when we revisit

Cheapest-to-most-expensive, each independently shippable:

1. **Const-divisor `Fdiv` → `Fmul` rewrite** in `const_fold.rs`
   (the "bigger lever" subsection above). Lands without the inliner,
   eliminates `divu` for any literal divisor. ~50-100 LOC.
2. Local CSE + algebraic peephole + extended const-fold as one pass.
   ~300 LOC. Modest standalone, bigger after inliner.
3. Stack-slot store-to-load forwarding. Likely the single biggest win
   for the inlined `render()` shape.
4. Mark imports pure where applicable (`@std.math::sin`, etc.). Enables
   later passes to hoist them.
5. Restricted LICM: outermost loop only, pure scalar arithmetic only,
   skip loads + impure calls. Build per-loop `mod_set: Vec<bool>` in one
   linear scan. Combined with (1), captures the uniform-divisor case for
   free.
6. Inline `__lp_lpir_fdiv_recip_q32` (the small story above) — orthogonal,
   can land any time. Note this stops mattering much if (1)+(5) land first.
7. _Then_ if more is needed: build an SSA layer (Braun's algorithm — small
   and fits a small codebase better than Cytron). Opens the door to IV
   analysis, full GVN, SCCP, the rest.

Steps 1-6 are non-SSA, each in the same complexity ballpark as
`const_fold.rs`. Step 7 is a different magnitude of investment.

## Why not now

- No one cares about perf on an unreleased app.
- Inliner isn't merged (code size). Most of the wins above only
  materialize after inlining; doing them first means optimizing code
  shapes that won't exist in the shipping pipeline.
- Cranelift's own optimizer covers some of this for the host JIT path.
  The native lpvm backend is the one that benefits most, and it's still
  a moving target.
- Real features have higher leverage on the product than micro-perf.

## Pointers for whoever picks this up

- Inliner branch: ask in chat (was on another branch as of 2026-04-19).
- Existing pass to model new ones on:
  [`lp-shader/lpir/src/const_fold.rs`](../../lp-shader/lpir/src/const_fold.rs).
- LPIR design constraints:
  [`docs/design/lpir/00-overview.md`](../design/lpir/00-overview.md)
  (esp. decisions 6 and 7 on non-SSA + structured control flow,
  and the "LPIR optimizations" entry in
  [`09-future.md`](../design/lpir/09-future.md)).
- Q32 numeric guarantees that make LICM speculation safe:
  same overview doc, "Numeric semantics summary" section.
- Profile that prompted this:
  [`profiles/2026-04-19T22-56-15--examples-basic--steady-render--wrapping-reciprocal/report.txt`](../../profiles/2026-04-19T22-56-15--examples-basic--steady-render--wrapping-reciprocal/report.txt).

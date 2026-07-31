# Float — f32 (IEEE-754 binary32) Semantics

This document is the single source of truth for **f32 numeric semantics** in
LightPlayer — the sibling of [`q32.md`](./q32.md) for the Float side of the
`float_mode` slot. All f32 execution tiers (LPIR interpreter, `lpvm-wasm`,
`lpvm-native` hardware and soft float, and — within its already-documented
latitude — the GPU tier) conform to it.

> **Posture.** Every choice below errs toward **performance and ease of
> implementation**: we guarantee exactly what every target's native f32 path
> already produces for free, mark as *target-defined* what hardware disagrees
> on and software cannot cheaply unify, and mark as *unspecified* what GLSL
> itself leaves undefined. We never insert per-operation fixup code to
> manufacture agreement the platforms don't natively have.
>
> **Related:** `docs/adr/2026-07-08-glsl-canonical-builtins.md` (builtin
> algorithm semantics), `docs/adr/2026-07-09-preview-fidelity-tiers.md` (GPU
> tier latitude), `docs/design/q32.md` (Fixed mode).

## 1. Conformance classes

Every f32 behavior falls into exactly one class:

| Class | Meaning | Corpus/testing rule |
|---|---|---|
| **Guaranteed** | Identical, bit-exact result on every conforming target | Filetests assert freely |
| **Target-defined** | Each target has one fixed, documented behavior; targets may differ | Recorded per target (Xtensa: the M6 FP-contract ADR); cross-target assertions forbidden |
| **Unspecified** | Any result permitted, may vary between runs of different builds | Never asserted; authors must not depend on it |

The conformance oracle is `interp.f32`. **Where the oracle violates a
Guaranteed row, the oracle is wrong and gets fixed** — the classes below are
normative, not descriptive of any one implementation.

## 2. Representation and environment

- Values are IEEE-754 **binary32**. No double promotion anywhere in the tier.
- **Rounding is round-to-nearest-even, always.** There is no dynamic rounding
  mode, no `fcsr`/FCR access from shader code, and emitters never change the
  mode. (Every target defaults to RNE; exposing mode switching would cost
  state save/restore on every boundary for a feature GLSL doesn't have.)
- **No floating-point exceptions or observable status flags.** Invalid
  operations produce NaN and continue; nothing traps.

## 3. Guaranteed

These are bit-exact everywhere because every target's native path already
agrees:

- **`+`, `-`, `*`, `/`, `sqrt`** — correctly rounded (RNE) per IEEE-754.
- **Special values in arithmetic**: `x/0 = ±inf` (for finite `x ≠ 0`),
  `0/0 = NaN`, `inf - inf = NaN`, `inf * 0 = NaN`, and NaN propagates through
  arithmetic. A NaN operand never silently becomes a finite number in
  `+ - * / sqrt`.
- **Negate and abs** are sign-bit operations: exact, and apply to NaN and ±0
  without normalizing them.
- **Comparisons** (`< <= > >= == !=`): IEEE semantics. Any ordered comparison
  with NaN is `false`; `!=` with NaN is `true`; `0.0 == -0.0` is `true`.
- **Signed zero** is preserved by arithmetic per IEEE (e.g. `-1.0 * 0.0`
  is `-0.0`).
- **`i32 ↔ f32` conversions**: int→float is correctly rounded (RNE).
  float→int truncates toward zero; **finite out-of-range values saturate** to
  `i32::MIN`/`i32::MAX`.
- **`floor`, `ceil`, `trunc`** — exact (every result is representable).
- **`fma` where spelled explicitly by a builtin contract** is a single
  rounding. (Contraction of *separate* `a*b+c` expressions is target-defined —
  see §4.)

## 4. Target-defined

One fixed behavior per target, documented per target, allowed to differ
between targets. The Xtensa entries are established by the M6 hardware
conformance campaign and recorded in its FP-contract ADR; wasm's come from
the WebAssembly spec; RV32F's from the RISC-V F spec.

- **Denormal (subnormal) handling.** A target may flush denormal inputs
  and/or outputs to zero. wasm and RV32F preserve denormals (their specs
  require it); the ESP32-S3 FPU and typical GPUs flush. Unifying this would
  mean per-op software fixup on the hot path — rejected. Consequence:
  results are only portable down to `~1.2e-38` magnitude; below that, a
  target may produce `0.0` where another produces a tiny nonzero value.
- **Expression contraction.** An emitter may (or may not) fuse `a*b + c`
  into a fused multiply-add with a single rounding. Shaders must not depend
  on the intermediate rounding of a multiply feeding an add.
- **NaN bit patterns.** Which NaN (payload, sign, quiet bit) an operation
  produces or propagates. "Is NaN" is portable; *which* NaN never is.
- **`round()` ties.** Halfway cases round to even or away from zero,
  per target — this is GLSL's own latitude, and hardware disagrees.

## 5. Unspecified

Any result is permitted. The corpus never asserts these; authors must treat
them as "don't do that":

- **`min` / `max` / `clamp` with a NaN operand.** IEEE-754 itself defines
  competing operations (2008 `maxNum` returns the number; 2019 `maximum`
  returns the NaN), targets natively implement different ones (Rust/RISC-V
  favor the number, wasm propagates the NaN), and GLSL declares the case
  undefined. Each target uses its native instruction. With no NaN operand,
  `min`/`max` are Guaranteed (including their behavior on mixed ±0 being
  target-native — do not depend on which zero wins).
- **float→int conversion of NaN.** Saturation applies to finite values only;
  NaN converts to an unspecified integer (targets natively produce `0`,
  `i32::MAX`, or `i32::MIN`).
- **GLSL-undefined library inputs**: `normalize(vec3(0))`, `inversesqrt(0)`,
  `pow` with negative base, `log` of non-positive values, `asin`/`acos`
  outside `[-1, 1]`, and the rest of the GLSL "undefined if" catalog. These
  return *some* f32 (often NaN or inf) and never trap, but the value is not
  portable.

## 6. Builtins and transcendentals

The lpfn builtin library (noise, color, …) and the GLSL transcendentals
(`sin`, `cos`, `exp`, `pow`, `atan`, …) are **approximations, not
correctly-rounded IEEE operations**. Their algorithmic semantics are defined
by the canonical GLSL sources (`lp-shader/lps-builtins/glsl/`, per the
canonical-builtins ADR); each f32 implementation must agree with the
canonical definition **within a documented conformance tolerance**, checked
by `lps-filetests`' conformance suite. Speed is prioritized over ulp
accuracy — the precedent is Q32's parabolic `sin` (3× the speed of the
Taylor version it replaced).

Inside builtin implementations, the §3–§5 classes still apply to the
individual operations; a builtin's overall tolerance band is what conformance
asserts.

## 7. Authoring guidance

For shader authors (human and agentic):

- Don't depend on NaN identities — test with `isnan`-style patterns rather
  than `x != x` tricks through `min`/`max`.
- Don't depend on values below `~1.2e-38`; a device may flush them to zero.
- Don't depend on which zero (`+0`/`-0`) survives `min`/`max`, or on
  intermediate rounding of `a*b + c`.
- Division by zero and out-of-domain library calls won't crash the shader,
  but their results aren't portable — guard them.

## 8. Testing rules (summary for `lps-filetests`)

1. Assert Guaranteed behavior freely, on every f32 target.
2. Target-defined behavior may be asserted **per target only** (e.g. an
   `xtn.f32`-scoped expectation pinned by the M6 silicon campaign), never
   cross-target.
3. Unspecified behavior is never asserted. Edge-case files exist to verify
   the *Guaranteed* rows (NaN propagates through `+`, comparisons are false,
   conversions saturate) — not to pin the unspecified ones.
4. The oracle (`interp.f32`) is subordinate to this document: an oracle
   result that contradicts a Guaranteed row is an oracle bug (tracked for
   fixing in milestone M8 of the f32 roadmap).

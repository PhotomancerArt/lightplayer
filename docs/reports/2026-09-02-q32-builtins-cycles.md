# Q32 math builtins: cycle census and bit-identical 32-bit exp / fsqrt

Date: 2026-09-02
Plan: `lp2025/2026-09-02-1942-q32-builtins-perf`
PR: https://github.com/PhotomancerArt/lightplayer/pull/502

## Summary

The Q32 math builtins were the largest slice of shader time on the ESP32-C6
cycle model: `exp` in `examples/meteor` (18% of the frame, all of it a
64-bit software division per series term) and `sqrt` / `sin` / `cos` /
`fdiv_recip` in `examples/zook-dome` (~35% of the frame together).

Shipped, both **bit-identical** to the previous implementations for every
`i32` input (exhaustive proofs, see below):

- `__lps_exp_q32`: the per-term `fdiv_q32` (an `i64` divide, `__udivdi3`
  on RV32) is exactly `in_value / n`, and that quotient is computed with an
  exact reciprocal table and one `mulhu`. Census at `x = −6` (the meteor
  tail): **3,854 → 760 cycles/call** (5.1×); worst case `x = 10`: 6,009 → 1,082.
- `__lp_lpir_fsqrt_q32`: `u64::isqrt` (software clz, three `divu`/`remu`
  pairs and a `__udivdi3` path) replaced by an exact 32-bit floor root: even
  shift normalise, 256-entry Q15 reciprocal-sqrt seed, two multiply-only
  Newton steps, exact ±1 correction. **367 → 63 cycles/call** on the census
  image (~180 → 63 against the O3 profile), 56 straight-line instructions.

Not shipped, measured at the floor of their algorithms: `sin`/`cos` (~80
cycles, no divide, no call) and `fdiv_recip` (~60 cycles, half of it one
hardware `divu`).

Pending decision (D1, ship gate): a range-reduced `exp` that is not
bit-identical — 110 cycles flat and 1.5–3× more accurate than the series
for |x| ≥ 4. Numbers in §8.

No hardware was run; every number is the RV32 emulator under
`CycleModel::Esp32C6` (`lp-emu-core`), which charges 1 cycle for ALU/`mul`,
32 for `div`/`rem`, 2 for loads and taken branches, 3 for `jalr`.

## Method

- **Census** (`cargo test -p lps-filetests --release builtin_cycle_census
  -- --ignored --nocapture`, `LP_CENSUS_DETAIL=1` for per-input rows): one
  GLSL wrapper per builtin behind the corpus's runtime-opaque `rt(x)`,
  compiled for `rv32n.q32`, swept over a fixed input set, wrapper overhead
  (`ident` 63 cycles / `ident2` 111) subtracted. It runs on the filetests
  image, which `scripts/build-builtins.sh` compiles at `opt-level=1`.
- **Profiles** (`cargo run -q -p lp-cli -- profile examples/<x> --collect
  cpu,alloc --mode steady-render`, then `profile function <dir> <symbol>`):
  the `fw-emu` firmware under `release-emu`, which — like the device's
  `release-esp32` — compiles `lps-builtins` at `opt-level=3`
  (`[profile.release.package.lps-builtins]`). Profile numbers are on the
  product's codegen; census numbers rank and measure deltas. The two agree
  within 10% for `sin`, `cos`, `fdiv_recip` and `exp`; the O1 image's
  `u64::isqrt` was 2× the O3 one, so the sqrt baseline is quoted from both.
- **Proofs**: each rewritten file keeps its former body as a test-only
  reference and compares every `i32` input (`cargo test -p lps-builtins
  --release exhaustive -- --ignored --nocapture`); a sampled version runs in
  the default suite.

## Baseline ranking

| builtin | workload | calls | cycles/call (profile, O3) | census (O1) | share of frame |
|---|---|---:|---:|---:|---:|
| `__lps_exp_q32` | meteor | 474 | 3,940 (≈19 `__udivdi3` × 164) | 3,854 @ −6 | 17.9% |
| `__lp_lpir_fsqrt_q32` | zook-dome | 6,000 | ~180 | 367 | 8.8% |
| `__lps_sin_q32` | zook-dome | 6,000 | 83 | 79 | 4.4% |
| `__lps_cos_q32` | zook-dome | 12,000 | ~80 | 82 | 8.1% |
| `__lp_lpir_fdiv_recip_q32` | zook-dome | 24,000 | 63 | 57 | 13.4% |
| `[jit] render_2d` self | zook-dome | 6,000 | 370 | — | 19.6% |

Zook-dome per pixel ≈ 1,037 cycles: 3 divides + 1 sqrt + 2 cos + 1 sin ≈
610 in builtins, 370 in JIT code.

## exp — bit-identical 32-bit series

Inside the series `in_value > 0`, so `fdiv_q32(in_value, n << 16)` — the
`i64` quotient of two positive multiples of 2^16 — is exactly `in_value / n`
and never saturates. With `M = ⌈2^32/n⌉` and `e = M·n − 2^32 < n`,
`⌊x·M/2^32⌋ = ⌊x/n⌋` whenever `x·e < 2^32`; `x ≤ 772,242 < 2^20` and
`n ≤ 29`, so a 30-entry `u32` table and one `mulhu` give the exact quotient
(the condition is asserted per entry at compile time). The negative-x
reciprocal `fdiv_q32(ONE, r) = ⌊2^32/r⌋` with `r ≥ 65537` is
`u32::MAX / r + [r is a power of two]`. Everything else — saturating
product, saturating accumulation, termination test, special cases — is
unchanged.

| input | before | loop + `divu` | reciprocal table (shipped) |
|---|---:|---:|---:|
| −0.5 | 700 | 360 | 247 |
| −2 | 1,894 | 694 | 437 |
| −6 (meteor tail) | 3,854 | 1,280 | **760** |
| 10 (worst) | 6,009 | 1,892 | 1,082 |

Unrolling was not needed: the table gives the exact `mulhu` quotient inside
the loop. 110 RV32 instructions (was 133); no `__udivdi3`/`__divdi3`.

Proof: 4,294,967,296 inputs bit-identical to the former body, 1.7–6 s on 12
threads (2026-09-02).

## fsqrt — exact 32-bit floor root

`floor(sqrt(x·2^16))` is the unique `c` with `c² ≤ n < (c+1)²`, so any
method that lands on it is bit-identical. The shipped one: shift `x` left
by an even amount into `[2^30, 2^32)` (4-step branchless ladder — RV32IMAC
has no `clz`), seed `1/√M` from a 256×u16 Q15 table (512 B rodata,
generated by a `const fn` from integers), two Newton steps
`R ← R(3 − M R²)/2` with every product a `mulhu` and one Q26 unit taken off
so `R` never exceeds `1/√M`, then `y = mulhi(m, R) << 2`, shift back to
final scale, and walk up while `(c+1)² ≤ n` comparing `(hi, lo)` word pairs.
The approximation only sets how many corrections run — the proof reports a
maximum of **one**.

56 RV32 instructions, no calls, no divides (was 264 at O1 with three
`divu`/`remu` pairs and a `__udivdi3` path). Census 367 → **63** cycles
(70 with one correction); `inversesqrt` 426 → 122 (its own `fdiv_q32`
`__udivdi3` remains — follow-up).

Proof: 4,294,967,296 inputs, zero mismatches, max 1 correction step, 22 s on
12 threads (2026-09-02).

## sin / cos / fdiv_recip — at floor

| symbol | O3 instrs | calls / div | cycles/call | verdict |
|---|---:|---|---:|---|
| `__lps_sin_q32` | 78 | none | ~80 | exact `mulh`-magic range reduction + four Q16.16 products; nothing bit-identical saves ≥ 5 cycles |
| `__lps_cos_q32` | 81 | none (sin inlined after the `+π/2`) | ~80 | already one flat function |
| `__lp_lpir_fdiv_recip_q32` | 23 | one `divu` | ~60 | the divide is half the cost; a division-free exact `⌊2^31/d⌋` costs more instructions than it saves |

Levers left are not builtins work: inlining `fdiv_recip` in `lpvm-native`
(the ~15-cycle call: argument moves + `jalr`/`ret`; the 2026-05 attempt was
reverted on a filetest failure), and a result-changing sine restructure
`x·(B + C·|x|)` worth ~15%.

## Before / after profiles

Same commands as the baseline (`--note after`); the profiler renders the
same frames on the same cycle model, so the deltas are exact, not sampled.

| workload | metric | before | after | delta |
|---|---|---:|---:|---:|
| meteor | total attributed cycles (whole run, 8 frames) | 10,432,613 | 8,765,715 | −16.0% |
| meteor | `__lps_exp_q32` inclusive (474 calls) | 1,867,413 | 237,347 | −87.3% |
| meteor | `__lps_exp_q32` cycles/call (O3) | 3,940 | 501 | 7.9× |
| meteor | `__udivdi3` calls | 9,051 | 0 | — |
| meteor | `[jit] render_2d` inclusive (hottest profile entry) | 2,288,312 | 621,414 | −72.8% |
| meteor | `[jit] drawMeteor` inclusive (960 calls) | 939,539 | 264,484 | −71.9% |
| zook-dome | total attributed cycles (whole run) | 11,318,990 | 10,641,982 | −6.0% |
| zook-dome | `__lp_lpir_fsqrt_q32` inclusive (6,000 calls) | 995,372 | 414,300 | −58.4% |
| zook-dome | `__lp_lpir_fsqrt_q32` cycles/call (O3) | 166 | 69 | 2.4× |
| zook-dome | `[jit] render_2d` inclusive per pixel (6,000 calls) | 1,037 | 925 | −10.8% |
| zook-dome | `sin` / `cos` / `fdiv_recip` inclusive | 498,000 / 914,300 / 1,512,000 | unchanged | 0 |

Per frame, meteor's shader work (`render_2d` over 60 LEDs, four meteors)
drops from ≈ 286 k to ≈ 78 k cycles; on the 160 MHz C6 that is ≈ 1.8 ms →
0.5 ms per frame for the shader alone. Zook-dome (1,500 lamps) saves ≈
112 cycles per lamp ≈ 168 k cycles ≈ 1.05 ms per frame; its remaining
builtin time is three `fdiv_recip` (one per `pos / outputSize` lane plus
the beam) and the trig, all at floor — the next lever there is the backend
(inline divide) or the shader (`pos * (1.0 / outputSize)` as a uniform).

Profile directories (gitignored): `profiles/2026-09-02T19-36-*--*--baseline`
and `profiles/2026-09-02T20-3*--*--after` in the plan's worktree.

## D1 — accuracy-changing exp candidate (not shipped)

`exp(x) = 2^k · exp(f)`, `k = round(x/ln 2)`, `|f| ≤ ln2/2`, degree-5
Horner polynomial in Q16.16, same saturation and underflow thresholds
(`exp_q32_candidate_range_reduced` in `exp_q32.rs`'s test module; the
accuracy report is `d1_candidate_accuracy_report`).

Cycles (census, temporary swap, reverted): **110 @ x < 0, 113 @ x > 0,
flat**; 124 instructions, no calls, no divides. Versus the shipped series:
760 @ −6, 1,082 @ 10.

Accuracy against `f64::exp` over the whole domain (Q16.16 ulps; relative
error where `exp ≥ 1/16`):

| band | inputs | shipped max ulp | shipped mean ulp | shipped max rel | candidate max ulp | candidate mean ulp | candidate max rel |
|---|---:|---:|---:|---:|---:|---:|---:|
| \|x\| < 1 | 131,071 | 9 | 1.94 | 5.6e-5 | 5 | 1.30 | 4.4e-5 |
| 1 ≤ \|x\| < 4 | 393,216 | 130 | 12.7 | 2.3e-4 | 93 | 9.8 | 2.5e-4 |
| 4 ≤ \|x\| < 8 | 524,288 | 12,088 | 722 | 6.2e-5 | 7,203 | 402 | 3.9e-5 |
| \|x\| ≥ 8 | 405,058 | 161,747 | 11,796 | 7.6e-5 | 52,111 | 5,867 | 3.7e-5 |

Adoption cost: an ADR (accuracy decision), every Q32 filetest target re-run
(`rv32n`, `rv32lpn`, `rv32c`, `wasm`, `xtn`, `xtlpn`; 18 files touch the exp
family, all `~=` at ≥ 5e-3, so expectation edits are unlikely but must be
verified), lpfn snapshots unaffected (no lpfn builtin calls `exp`), the
shader-oracle CRC unaffected (that example does not call `exp`), and any
Studio story baseline that renders one of the seven examples using the exp
family (`basic`, `comet`, `fire2012`, `fyeah-*`, `meteor`, `rocaille`) may
move by a few ulps. A Q2.30 polynomial core would improve the candidate
further (not measured).

## Follow-ups

1. Exact 48/32 `__lp_lpir_fdiv_q32` (two `divu` steps, Knuth-D on 16-bit
   limbs) — still a `__udivdi3` inside `mod`, `tan`, `log`, `log2`, `pow`,
   `tanh`, `atan2`, `asin`, `inversesqrt`.
2. Inline `fdiv_recip` in `lpvm-native` (backend; correctness-first with
   the rv32 filetests, as the May report asked).
3. D1, if approved at the ship gate.
4. Device microbench of the new kernels when the C6 bench is free
   (`just fwtest-jit-math-perf-esp32c6`).
5. LICM of loop-invariant trig / reciprocals (middle-end).

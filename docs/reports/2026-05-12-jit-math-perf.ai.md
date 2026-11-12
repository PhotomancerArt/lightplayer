# JIT Math Perf Spike

Date: 2026-05-12

## Summary

This pass focused on the RV32/Q32 JIT hot-path math used during steady-state
rendering.

The shipped change is intentionally pragmatic:

- Make normal Q32 rendering fast by default:
  - add/sub: wrapping inline mode
  - mul: wrapping inline mode
  - div: reciprocal helper mode
- Replace the old Taylor `__lps_sin_q32` helper with a fast parabolic sine
  approximation.
- Update `sincos`/`cos` users and generated snapshots/LUTs to match the new
  sine.
- Add an ESP32-C6 firmware microbenchmark harness for future math candidates.

## ESP32-C6 Microbench Results

Command:

```sh
ESPFLASH_PORT=/dev/cu.usbmodem1101 just fwtest-jit-math-perf-esp32c6
```

Hardware: ESP32-C6 at 160 MHz.

Key medians:

| Kernel | Calls | Median cycles | Per call |
| --- | ---: | ---: | ---: |
| old Taylor sine reference | 41 | 13,382 | 326 |
| shipped `__lps_sin_q32` | 41 | 4,392 | 107 |
| parabolic candidate via harness fn boundary | 41 | 7,845 | 191 |
| cubic sine candidate | 41 | 4,647 | 113 |
| LUT nearest 256-2048 | 41 | 7,753 | 189 |
| LUT linear 256-2048 | 41 | 14,179-14,188 | 345-346 |
| saturating Q32 mul helper | 357 | 18,669 | 52 |
| wrapping Q32 mul | 357 | 9,216 | 25 |
| reciprocal div helper | 525 | 30,258 | 57 |
| inline reciprocal Rust candidate | 525 | 24,710 | 47 |

Quality versus the old Taylor sine reference:

- parabolic sine: `max_abs=405`, `mean_abs=71`
- cubic sine: `max_abs=133182`, `mean_abs=26102`
- LUT nearest improves with size but remains worse than parabolic at similar
  measured cost.
- LUT linear has good mean error but is slower than the old Taylor reference on
  this harness because interpolation dominates.

LUT access itself was not the blocker: rodata and RAM were effectively the same
on this test (`~12-15` cycles/access depending on access pattern). The full
LUT sine path is slower because of index/range/interpolation math, not because
flash lookup alone is surprisingly expensive.

## Steady Render: examples/basic

Baseline from user profile:

- total attributed cycles: `4,896,607`
- `__lps_sin_q32`: `554,720` self cycles
- `__lp_lpir_fdiv_recip_q32`: `694,080` self cycles

After fast defaults + fast sine:

Profile:
`profiles/2026-05-12T09-48-56--examples-basic--steady-render--jit-math-perf-fast-defaults`

- total attributed cycles: `4,291,616`
- delta: `-604,991` cycles, about `-12.4%`
- `__lps_sin_q32`: `154,240` self cycles
- `__lps_cos_q32`: `79,690` self cycles
- `__lp_lpir_fdiv_recip_q32`: `694,080` self cycles

The basic workload confirms the sine change is real at profile level. Division
is now the largest remaining math helper in that profile.

## Rocaille Trig Workload

Rocaille needed metadata refresh before profiling:

- current fixture mapping schema
- lowercase `color_order`
- texture `[size]`
- output/shader bus bindings

Profile:
`profiles/2026-05-12T10-09-51--examples-rocaille--steady-render--jit-math-perf-fast-trig`

Total attributed cycles: `53,274,752`

Top math self-cycle entries:

- `__lp_lpir_fdiv_recip_q32`: `14,055,120` (`26.4%`)
- `__lps_sin_q32`: `12,463,371` (`23.4%`)
- `__lps_cos_q32`: `2,861,152` (`5.4%`)
- `__lp_lpir_itof_s_q32`: `2,967,192` (`5.6%`)
- `__lp_lpir_fsqrt_q32`: `2,062,581` (`3.9%`)
- `__lps_tanh_q32`: `1,370,848` (`2.6%`)
- `__lps_sinh_q32`: `1,289,872` (`2.4%`)

Rocaille validates the focus: even after the sine helper speedup, trig and
divide dominate this shader.

## Inline Divide Experiment

The ESP32 harness suggested an inline reciprocal div candidate could save about
10 cycles/call versus the reciprocal helper.

I attempted an `lpvm-native` inline reciprocal lowering, then immediately ran
RV32 native filetests. The filetests caught a correctness bug:

- `10.0 / 3.0` produced `81.33313`

That experiment was reverted. The reciprocal helper remains the shipping path
for this pass. This is a good follow-up, but it needs a small, correctness-first
backend implementation with filetests before profiling again.

Relevant focused filetests after revert:

```sh
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/q32fast-div-recip-by-zero.glsl \
  scalar/float/op-divide.glsl \
  builtins/trig-sin.glsl \
  builtins/trig-cos.glsl
```

Result: `33/33` passed in `83ms`.

Cycle summaries:

- `builtins/trig-sin.glsl`: `140` estimated cycles across 10 runs
- `builtins/trig-cos.glsl`: `140` estimated cycles across 10 runs
- `scalar/float/op-divide.glsl`: `268` estimated cycles across 9 runs
- `q32fast-div-recip-by-zero.glsl`: `97` estimated cycles across 3 runs
- `q32fast-div-recip.glsl`: `75` estimated cycles for 1 run

## Follow-Ups

Best next targets, in order:

1. Inline reciprocal div correctly in `lpvm-native`, guarded by RV32 native
   filetests before profiling.
2. Add compiler-level `sincos`/`cos` specialization so call overhead and repeated
   range reduction do less damage in trig-heavy shaders.
3. Consider `tanh`/`sinh` approximation work for Rocaille-like shaders after the
   first two items.
4. Keep LUT sine out of the product path for now; it did not win on-device.

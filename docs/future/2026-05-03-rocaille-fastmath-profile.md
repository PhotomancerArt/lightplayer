# Rocaille fastmath shader profile

## Status: captured for future optimization

Notes from a 2026-05-03 profile of `examples/rocaille/src/rainbow.shader/main.glsl`
on the ESP32-C6 cycle model. This is a deliberately shader-heavy worst case for
the current scalar Q32 native backend: nested loops, many transcendental calls,
division in the inner loop, and no useful vector hardware on the target.

The useful comparison is the fastmath run:

`[profiles/2026-05-03T14-07-07--examples-rocaille--steady-render--fastmath/report.txt](../../profiles/2026-05-03T14-07-07--examples-rocaille--steady-render--fastmath/report.txt)`

The earlier saturating-math run is still useful as a contrast, but should not be
treated as the representative shader mode:

`[profiles/2026-05-03T13-57-38--examples-rocaille--steady-render/report.txt](../../profiles/2026-05-03T13-57-38--examples-rocaille--steady-render/report.txt)`

## What the shader stresses

The hot body is:

```glsl
for (int i = 1; i < ITERS; i++) {
    v = p;
    for (int f = 1; f < ITERS; f++) {
        float ff = float(f);
        v += sin(v.yx * ff + float(i) + phase) / ff;
    }

    vec4 ramp = cos(float(i) + vec4(0.0, 1.0, 2.0, 3.0)) + 1.0;
    color += ramp / 6.0 / max(length(v), 0.001);
}
```

With `ITERS = 10`, the inner loop runs 81 times per pixel. After scalarization,
that means a large number of Q32 `sin`, reciprocal division, add, multiply, and
sqrt/length operations per frame.

## Profile summary

Fastmath reduces the attributed steady-render cost substantially:


| Mode           | Attributed cycles | Steady frame event cost |
| -------------- | ----------------- | ----------------------- |
| Saturating Q32 | 30.49M            | ~7.58M cycles/frame     |
| Fastmath Q32   | 20.69M            | ~5.13M cycles/frame     |


The fastmath top self costs:


| Symbol                                              | Self cycles | Share |
| --------------------------------------------------- | ----------- | ----- |
| `__lps_sin_q32`                                     | 10.59M      | 51.2% |
| `__lp_lpir_fdiv_recip_q32`                          | 3.73M       | 18.0% |
| JIT block `0x80009dd8`                              | 2.07M       | 10.0% |
| `fixture_node::accumulate_fixture_channels` closure | 0.82M       | 4.0%  |
| `__lp_lpir_itof_s_q32`                              | 0.79M       | 3.8%  |
| `__lp_lpir_fsqrt_q32`                               | 0.53M       | 2.6%  |


Fastmath largely removes the old 64-bit saturating division cost. In the
saturating run, `__lp_lpir_fdiv_q32`, `__divdi3`, and `u64_div_rem` together
were a major part of the profile. In the fastmath run, division is still visible
through `__lp_lpir_fdiv_recip_q32`, but the remaining bottleneck is much clearer:
`__lps_sin_q32` dominates the frame.

## What this validates

- **Fastmath is the right default for performance investigations.** It cuts the
profile by about one third for this shader and prevents saturating division
from hiding other problems.
- **Fastmath should probably become the product default.** The saturating Q32
path is useful as a reference/debug mode, but it is not true IEEE GLSL
correctness and it costs enough to distort embedded performance. Prefer one
high-level math-mode toggle: fast/default for rendering, debug/reference for
differential testing and compiler investigations.
- **Avoiding runtime division still matters.** Reciprocal division is much
better than saturating division, but `__lp_lpir_fdiv_recip_q32` is still 18%
self time. Constant-divisor specialization and reciprocal precomputation remain
worthwhile.
- **Uniform and loop-invariant hoisting would help.** `phase` is uniform for the
frame, and `ramp = cos(float(i) + vec4(...)) + 1.0` is independent of pixel and
time. Today those are recomputed inside per-pixel shader execution.
- **The runtime graph is not the limiting factor here.** `ShaderNode::tick` and
native JIT rendering account for almost all inclusive time. Fixture sampling
and output pushing are visible but secondary.

## New pressure: trig quality and speed

This profile pushes harder on something the earlier middle-end notes did not
emphasize enough: Q32 trig speed.

`__lps_sin_q32` currently uses range reduction plus an accurate Taylor-style
fixed-point approximation. That is a reasonable correctness-first implementation,
but it is expensive for shader code that calls `sin` in an inner loop. In the
fastmath profile, `sin` alone is more than half of attributed cycles.

Future options to investigate:

- A shader-quality fast trig mode with lower accuracy but much lower latency.
- LUT or LUT-plus-polynomial approximations sized for embedded flash/cache
tradeoffs.
- Ben Hencke's MicroModSynth sine path is a useful concrete reference for the
LUT family: `[synth.c](https://github.com/simap/MicroModSynth/blob/main/src/synth.c)`
uses 8-bit or 16-bit sine tables with optional linear interpolation.
- Shared `sin/cos` range reduction and paired `sincos` lowering where source
patterns make it profitable.
- Special lowering for common periodic shader idioms.
- Hoisting or precomputing uniform trig calls after a pure-call/LICM pass exists.

## Why this is a useful worst case

This shader is not representative of every LED effect, but it is a good stress
test for the current architecture:

- It amplifies scalarization costs.
- It runs many fixed-point transcendental calls per pixel.
- It contains both literal and loop-varying division.
- It exposes the absence of LICM and uniform precomputation.
- It makes the ESP32-C6 no-vector-hardware ceiling obvious.

The takeaway is not that LightPlayer should chase a cross-pixel vector engine on
C6. The immediate lesson is narrower: once fastmath is enabled, the next large
scalar wins are trig approximation, reciprocal/constant division specialization,
and middle-end hoisting.
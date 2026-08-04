#!/usr/bin/env python3
"""Where does the residual A/B difference at t=100.1 come from?

Three bodies land just over the 1e-5 gate at the far end of the t-grid. For
each, recompute the single term that carries the timebase three ways:

  exact       f64 closed form
  original    the committed expression, every step rounded to f32
  converted   the rewrite, fed `fract(t / period)` rounded to f32 (what the
              ORACLE supplies)
  conv-ideal  the same rewrite fed an f64-accurate phase (what the engine's
              INTEGRATOR approximates — it accumulates rate*delta per tick
              and never evaluates `t / period`, so it does not inherit the
              cancellation that makes `fract` lossy at large t)

If `conv-ideal` is the closest of the three, the gap is a property of the
oracle's phase model plus the original's magnitude loss — not of the GLSL
rewrite. Disposable (P9).
"""

import math
import struct


def f32(x):
    return struct.unpack("<f", struct.pack("<f", float(x)))[0]


def fr(x):
    return x - math.floor(x)


TAU32 = f32(6.2831853)
T = 100.1


def report(name, exact, original, converted, ideal):
    print(
        f"{name:<30} exact={exact: .9f}"
        f"  orig={abs(original - exact):.3e}"
        f"  conv={abs(converted - exact):.3e}"
        f"  conv-ideal={abs(ideal - exact):.3e}"
    )


print("quad-strips / penta-strands: per-band chase head at t=100.1")
for band in range(4):
    rate = 0.25 + band * 0.1
    exact = fr(T * rate + band * 0.25)
    original = f32(fr(f32(f32(T) * f32(rate)) + f32(band * 0.25)))
    phase32 = f32(fr(f32(T) / f32(20.0)))
    phase64 = f32(fr(T / 20.0))
    converted = f32(fr(f32(phase32 * f32(5.0 + band * 2.0)) + f32(band * 0.25)))
    ideal = f32(fr(f32(phase64 * f32(5.0 + band * 2.0)) + f32(band * 0.25)))
    report(f"  band {band} (rate {rate:.2f} Hz)", exact, original, converted, ideal)

print()
print("fyeah-attract: wheel rotation and palette walk at t=100.1")
exact = fr(T * 0.115 * 2.0)
original = f32(fr(f32(f32(f32(T) * f32(0.115)) * f32(2.0))))
converted = f32(fr(f32(T) / f32(4.3478261)))
ideal = f32(fr(T / 4.3478261))
report("  wheel phase", exact, original, converted, ideal)

exact = (T * 0.055 * 2.0) % 3.0
original = f32(math.fmod(f32(f32(f32(T) * f32(0.055)) * f32(2.0)), 3.0))
converted = f32(f32(fr(f32(T) / f32(27.2727273))) * 3.0)
ideal = f32(f32(fr(T / 27.2727273)) * 3.0)
report("  palette phase (0..3)", exact, original, converted, ideal)

print()
print("smoke-project: sin(uv.x*16 + time*2.1) at t=100.1, uv.x=0.5")
exact = math.sin(0.5 * 16.0 + T * 2.1)
original = f32(math.sin(f32(f32(0.5 * 16.0) + f32(f32(T) * f32(2.1)))))
phase32 = f32(fr(f32(T) / f32(2.9919931)))
phase64 = f32(fr(T / 2.9919931))
converted = f32(math.sin(f32(f32(0.5 * 16.0) + f32(TAU32 * phase32))))
ideal = f32(math.sin(f32(f32(0.5 * 16.0) + f32(TAU32 * phase64))))
report("  wave A", exact, original, converted, ideal)

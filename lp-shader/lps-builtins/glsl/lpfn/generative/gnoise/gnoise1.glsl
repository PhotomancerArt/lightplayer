// lpfn_gnoise(float) — 1D value noise (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_gnoise(float, uint)`
// builtin, matching `src/builtins/lpfn/generative/gnoise/gnoise1_q32.rs`:
// random values at integer lattice points, cubic-smoothstep interpolation.
// Value/gradient lattice noise is a standard algorithm from graphics
// literature (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// The Q32 device implementation approximates the cubic smoothstep with a
// 256-entry LUT; the canonical uses the exact polynomial 3t^2 - 2t^3.
//
// NOTE: built on the chaotic sin-hash lpfn_random — conformance vs the Q32
// device implementation is statistical, not pointwise (see
// random/random1.glsl).
//
// Depends on: generative/random/random1.glsl

float lpfn_gnoise(float x, uint seed) {
    float i = floor(x);
    float f = x - i;

    float a = lpfn_random(i, seed);
    float b = lpfn_random(i + 1.0, seed);

    float u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u);
}

// lpfn_gnoise(vec2) — 2D value noise (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_gnoise(vec2, uint)`
// builtin, matching `src/builtins/lpfn/generative/gnoise/gnoise2_q32.rs`:
// random values at the four cell corners, bilinear interpolation with
// cubic-smoothstep weights. Standard lattice value noise (see
// docs/reports/2026-03-31-lpfx-license-audit.md; originally written with
// reference to LYGIA's gnoise.glsl).
//
// The Q32 device implementation approximates the cubic smoothstep with a
// 256-entry LUT; the canonical uses the exact polynomial 3t^2 - 2t^3.
//
// NOTE: built on the chaotic sin-hash lpfn_random — conformance vs the Q32
// device implementation is statistical, not pointwise (see
// random/random1.glsl).
//
// Depends on: generative/random/random2.glsl

float lpfn_gnoise(vec2 p, uint seed) {
    vec2 i = floor(p);
    vec2 f = fract(p);

    float a = lpfn_random(i, seed);
    float b = lpfn_random(i + vec2(1.0, 0.0), seed);
    float c = lpfn_random(i + vec2(0.0, 1.0), seed);
    float d = lpfn_random(i + vec2(1.0, 1.0), seed);

    vec2 u = f * f * (3.0 - 2.0 * f);

    // mix(a, b, u.x) + (c - a) * u.y * (1 - u.x) + (d - b) * u.x * u.y
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}

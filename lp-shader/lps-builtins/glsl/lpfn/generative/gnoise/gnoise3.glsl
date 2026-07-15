// lpfn_gnoise(vec3) — 3D value noise (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_gnoise(vec3, uint)`
// builtin, matching `src/builtins/lpfn/generative/gnoise/gnoise3_q32.rs`:
// random values at the eight cell corners, trilinear interpolation with
// quintic-smoothstep weights, remapped from [0, 1] to [-1, 1].
// Standard lattice value noise (see
// docs/reports/2026-03-31-lpfx-license-audit.md).
//
// The Q32 device implementation approximates the quintic smoothstep with a
// 256-entry LUT; the canonical uses the exact polynomial
// 6t^5 - 15t^4 + 10t^3.
//
// NOTE: built on the chaotic sin-hash lpfn_random — conformance vs the Q32
// device implementation is statistical, not pointwise (see
// random/random1.glsl).
//
// Depends on: generative/random/random3.glsl

float lpfn_gnoise(vec3 p, uint seed) {
    vec3 i = floor(p);
    vec3 f = fract(p);

    vec3 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    float c000 = lpfn_random(i, seed);
    float c100 = lpfn_random(i + vec3(1.0, 0.0, 0.0), seed);
    float c010 = lpfn_random(i + vec3(0.0, 1.0, 0.0), seed);
    float c110 = lpfn_random(i + vec3(1.0, 1.0, 0.0), seed);
    float c001 = lpfn_random(i + vec3(0.0, 0.0, 1.0), seed);
    float c101 = lpfn_random(i + vec3(1.0, 0.0, 1.0), seed);
    float c011 = lpfn_random(i + vec3(0.0, 1.0, 1.0), seed);
    float c111 = lpfn_random(i + vec3(1.0, 1.0, 1.0), seed);

    float x00 = mix(c000, c100, u.x);
    float x10 = mix(c010, c110, u.x);
    float x01 = mix(c001, c101, u.x);
    float x11 = mix(c011, c111, u.x);

    float y0 = mix(x00, x10, u.y);
    float y1 = mix(x01, x11, u.y);

    float result = mix(y0, y1, u.z);

    // Remap [0, 1] -> [-1, 1].
    return -1.0 + 2.0 * result;
}

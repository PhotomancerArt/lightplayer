// lpfn_random(vec3) — 3D sin-based random (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_random(vec3, uint)`
// builtin, matching `src/builtins/lpfn/generative/random/random3_q32.rs`.
//
// fract(sin(dot(p, K)) * 43758.5453123). Credit: David Hoskins (MIT) via
// LYGIA generative/random.glsl
// (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Seed semantics: raw seed word added to the Q16.16 angle = seed * 2^-16
// radians of phase (see random1.glsl).
//
// NOTE: chaotic sin-hash — conformance vs the Q32 device implementation is
// statistical, not pointwise (see random1.glsl).

float lpfn_random(vec3 p, uint seed) {
    float d = dot(p, vec3(70.9898, 78.233, 32.4355));
    float combined = d + float(seed) * (1.0 / 65536.0);
    return fract(sin(combined) * 43758.5453123);
}

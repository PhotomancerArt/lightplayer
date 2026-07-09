// lpfn_random(vec2) — 2D sin-based random (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_random(vec2, uint)`
// builtin, matching `src/builtins/lpfn/generative/random/random2_q32.rs`.
//
// fract(sin(dot(p, K)) * 43758.5453). Credit: MIT License (MIT)
// Copyright 2014, David Hoskins; distributed by LYGIA in
// generative/random.glsl (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Seed semantics: raw seed word added to the Q16.16 angle = seed * 2^-16
// radians of phase (see random1.glsl).
//
// NOTE: chaotic sin-hash — conformance vs the Q32 device implementation is
// statistical, not pointwise (see random1.glsl).

float lpfn_random(vec2 p, uint seed) {
    float d = dot(p, vec2(12.9898, 78.233));
    float combined = d + float(seed) * (1.0 / 65536.0);
    return fract(sin(combined) * 43758.5453);
}

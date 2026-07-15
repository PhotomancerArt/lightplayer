// lpfn_srandom(vec2) — 2D signed random in [-1, 1] (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_srandom(vec2, uint)`
// builtin, matching `src/builtins/lpfn/generative/srandom/srandom2_q32.rs`.
// Trivial transform of lpfn_random: -1 + 2 * random(p, seed) — basic
// arithmetic applied to our MIT-licensed random
// (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Depends on: generative/random/random2.glsl

float lpfn_srandom(vec2 p, uint seed) {
    return -1.0 + 2.0 * lpfn_random(p, seed);
}

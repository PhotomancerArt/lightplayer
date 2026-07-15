// lpfn_srandom(float) — 1D signed random in [-1, 1] (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_srandom(float, uint)`
// builtin, matching `src/builtins/lpfn/generative/srandom/srandom1_q32.rs`.
// Trivial transform of lpfn_random: -1 + 2 * random(x, seed).
//
// Depends on: generative/random/random1.glsl

float lpfn_srandom(float x, uint seed) {
    return -1.0 + 2.0 * lpfn_random(x, seed);
}

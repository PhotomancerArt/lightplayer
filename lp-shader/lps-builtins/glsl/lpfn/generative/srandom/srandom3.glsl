// lpfn_srandom(vec3) — 3D signed random in [-1, 1] (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_srandom(vec3, uint)`
// builtin, matching `src/builtins/lpfn/generative/srandom/srandom3_q32.rs`.
// Trivial transform of lpfn_random: -1 + 2 * random(p, seed).
//
// Depends on: generative/random/random3.glsl

float lpfn_srandom(vec3 p, uint seed) {
    return -1.0 + 2.0 * lpfn_random(p, seed);
}

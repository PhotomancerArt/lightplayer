// lpfn_srandom3_vec — 3D signed random returning vec3 (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_srandom3_vec(vec3, uint)`
// builtin, matching `src/builtins/lpfn/generative/srandom/srandom3_vec_q32.rs`.
//
// Three independent sin-hash channels with the classic gradient-noise dot
// constants (127.1/311.7/74.7 family). The constants and sin-hash pattern
// are the widely used public one-liner (David Hoskins lineage, MIT; see
// docs/reports/2026-03-31-lpfx-license-audit.md).
//
// NOTE: the seed parameter is accepted but unused, matching the Rust
// implementation (`_seed`); seeding this variant is future work tracked with
// the Q32 implementation.
//
// NOTE: chaotic sin-hash — conformance vs the Q32 device implementation is
// statistical, not pointwise (see random/random1.glsl).

vec3 lpfn_srandom3_vec(vec3 p, uint seed) {
    float dx = dot(p, vec3(127.1, 311.7, 74.7));
    float dy = dot(p, vec3(269.5, 183.3, 246.1));
    float dz = dot(p, vec3(113.5, 271.9, 124.6));
    vec3 r = fract(sin(vec3(dx, dy, dz)) * 43758.5453123);
    return -1.0 + 2.0 * r;
}

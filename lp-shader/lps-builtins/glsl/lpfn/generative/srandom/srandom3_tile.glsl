// lpfn_srandom3_tile — tiling 3D signed random vec3 (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer
// `lpfn_srandom3_tile(vec3, float, uint)` builtin, matching
// `src/builtins/lpfn/generative/srandom/srandom3_tile_q32.rs`:
// wrap the lattice coordinate with mod(p, tileLength), then evaluate
// lpfn_srandom3_vec on the wrapped coordinate.
//
// Depends on: generative/srandom/srandom3_vec.glsl

vec3 lpfn_srandom3_tile(vec3 p, float tileLength, uint seed) {
    vec3 wrapped = mod(p, vec3(tileLength));
    return lpfn_srandom3_vec(wrapped, seed);
}

// lpfn_hash — integer hash functions for noise generation (canonical f32/uint semantics).
//
// This is the canonical GLSL source for the LightPlayer `lpfn_hash` builtins.
// It is a direct port of the Rust implementation in
// `src/builtins/lpfn/hash.rs` (all arithmetic is exact uint math, so the
// canonical and the device implementation agree bit-for-bit).
//
// Algorithm credit: noiz library (github.com/ElliottjPierce/noiz), MIT license.
// The bit-mixing pattern is inspired by https://nullprogram.com/blog/2018/07/31/.
// See docs/reports/2026-03-31-lpfx-license-audit.md.

// Core hash: rotate/xor/multiply mixing with a large prime (249222277).
uint lpfn_hash_mix(uint x, uint seed) {
    // x ^= x.rotate_right(17)
    x ^= (x >> 17) | (x << 15);
    x *= 249222277u;
    // x ^= x.rotate_right(11) ^ seed
    x ^= ((x >> 11) | (x << 21)) ^ seed;
    x *= 249222277u;
    return x;
}

// 1D hash.
uint lpfn_hash(uint x, uint seed) {
    return lpfn_hash_mix(x, seed);
}

// 2D hash: combine coordinates non-commutatively, then mix.
uint lpfn_hash(uvec2 xy, uint seed) {
    uint cy = xy.y ^ 102983473u;
    // (x ^ 983742189) + (y ^ 102983473).rotate_left(8)
    uint combined = (xy.x ^ 983742189u) + ((cy << 8) | (cy >> 24));
    return lpfn_hash_mix(combined, seed);
}

// 3D hash: combine coordinates non-commutatively, then mix.
uint lpfn_hash(uvec3 xyz, uint seed) {
    uint cy = xyz.y ^ 102983473u;
    uint cz = xyz.z ^ 189203473u;
    // (x ^ 983742189) + (y ^ 102983473).rol(8) + (z ^ 189203473).rol(16)
    uint combined = (xyz.x ^ 983742189u) + ((cy << 8) | (cy >> 24)) + ((cz << 16) | (cz >> 16));
    return lpfn_hash_mix(combined, seed);
}

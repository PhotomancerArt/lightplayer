// lpfn_gnoise(vec3, float tileLength) — tilable 3D gradient noise
// (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer
// `lpfn_gnoise(vec3, float, uint)` builtin, matching
// `src/builtins/lpfn/generative/gnoise/gnoise3_tile_q32.rs`: gradients from
// lpfn_srandom3_tile at the eight cell corners, dotted with corner offsets,
// trilinear interpolation with quintic weights, normalized to [0, 1].
// tileLength == 0 falls back to (non-tiling) lpfn_gnoise(vec3) remapped to
// [0, 1]. Standard tilable lattice gradient noise (see
// docs/reports/2026-03-31-lpfx-license-audit.md).
//
// NOTE: built on the chaotic sin-hash lpfn_srandom3_vec — conformance vs the
// Q32 device implementation is statistical, not pointwise (see
// random/random1.glsl).
//
// Depends on: generative/gnoise/gnoise3.glsl,
//             generative/srandom/srandom3_tile.glsl

float lpfn_gnoise(vec3 p, float tileLength, uint seed) {
    if (tileLength == 0.0) {
        // Normalize gnoise3 output from [-1, 1] to [0, 1].
        return lpfn_gnoise(p, seed) * 0.5 + 0.5;
    }

    vec3 i = floor(p);
    vec3 f = fract(p);

    vec3 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    // Matches the Rust port: tileLength * lacunarity(2.0) * 0.5 == tileLength.
    float scaledTile = tileLength;

    vec3 f000 = f - vec3(0.0, 0.0, 0.0);
    vec3 f100 = f - vec3(1.0, 0.0, 0.0);
    vec3 f010 = f - vec3(0.0, 1.0, 0.0);
    vec3 f110 = f - vec3(1.0, 1.0, 0.0);
    vec3 f001 = f - vec3(0.0, 0.0, 1.0);
    vec3 f101 = f - vec3(1.0, 0.0, 1.0);
    vec3 f011 = f - vec3(0.0, 1.0, 1.0);
    vec3 f111 = f - vec3(1.0, 1.0, 1.0);

    vec3 g000 = lpfn_srandom3_tile(i + vec3(0.0, 0.0, 0.0), scaledTile, seed);
    vec3 g100 = lpfn_srandom3_tile(i + vec3(1.0, 0.0, 0.0), scaledTile, seed);
    vec3 g010 = lpfn_srandom3_tile(i + vec3(0.0, 1.0, 0.0), scaledTile, seed);
    vec3 g110 = lpfn_srandom3_tile(i + vec3(1.0, 1.0, 0.0), scaledTile, seed);
    vec3 g001 = lpfn_srandom3_tile(i + vec3(0.0, 0.0, 1.0), scaledTile, seed);
    vec3 g101 = lpfn_srandom3_tile(i + vec3(1.0, 0.0, 1.0), scaledTile, seed);
    vec3 g011 = lpfn_srandom3_tile(i + vec3(0.0, 1.0, 1.0), scaledTile, seed);
    vec3 g111 = lpfn_srandom3_tile(i + vec3(1.0, 1.0, 1.0), scaledTile, seed);

    float d000 = dot(g000, f000);
    float d100 = dot(g100, f100);
    float d010 = dot(g010, f010);
    float d110 = dot(g110, f110);
    float d001 = dot(g001, f001);
    float d101 = dot(g101, f101);
    float d011 = dot(g011, f011);
    float d111 = dot(g111, f111);

    float x00 = mix(d000, d100, u.x);
    float x10 = mix(d010, d110, u.x);
    float x01 = mix(d001, d101, u.x);
    float x11 = mix(d011, d111, u.x);

    float y0 = mix(x00, x10, u.y);
    float y1 = mix(x01, x11, u.y);

    float result = mix(y0, y1, u.z);

    // Normalize to [0, 1].
    return result * 0.5 + 0.5;
}

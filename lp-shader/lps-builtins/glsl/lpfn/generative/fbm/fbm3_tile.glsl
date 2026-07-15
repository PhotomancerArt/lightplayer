// lpfn_fbm(vec3, float tileLength) — tilable 3D FBM (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer
// `lpfn_fbm(vec3, float, int, uint)` builtin, matching
// `src/builtins/lpfn/generative/fbm/fbm3_tile_q32.rs`: normalized octave sum
// of tilable gradient noise with persistence 0.5 and lacunarity 2.0.
// FBM is a standard procedure (see
// docs/reports/2026-03-31-lpfx-license-audit.md).
//
// NOTE: built on the chaotic sin-hash noise stack — conformance vs the Q32
// device implementation is statistical, not pointwise (see
// random/random1.glsl).
//
// Depends on: generative/gnoise/gnoise3_tile.glsl

float lpfn_fbm(vec3 p, float tileLength, int octaves, uint seed) {
    float persistence = 0.5;
    float lacunarity = 2.0;

    float amplitude = 0.5;
    float total = 0.0;
    float normalization = 0.0;
    vec3 pos = p;

    for (int i = 0; i < octaves; i++) {
        float scaledTile = tileLength * lacunarity * 0.5;
        float noiseValue = lpfn_gnoise(pos, scaledTile, seed);
        total += noiseValue * amplitude;
        normalization += amplitude;
        amplitude *= persistence;
        pos = pos * lacunarity;
    }

    return total / normalization;
}

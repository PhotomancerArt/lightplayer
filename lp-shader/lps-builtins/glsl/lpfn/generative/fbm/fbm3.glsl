// lpfn_fbm(vec3) — 3D fractal Brownian motion (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_fbm(vec3, int, uint)`
// builtin, matching `src/builtins/lpfn/generative/fbm/fbm3_q32.rs`:
// octave sum of lpfn_snoise with amplitude 0.5, gain 0.5, lacunarity 2.0.
// FBM (weighted octave sum) is a standard procedure from Perlin's 1985
// paper (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Depends on: generative/snoise/snoise3.glsl (which depends on hash.glsl)

float lpfn_fbm(vec3 p, int octaves, uint seed) {
    float value = 0.0;
    float amplitude = 0.5;
    vec3 pos = p;
    for (int i = 0; i < octaves; i++) {
        value += amplitude * lpfn_snoise(pos, seed);
        pos = pos * 2.0;
        amplitude *= 0.5;
    }
    return value;
}

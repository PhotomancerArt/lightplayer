// lpfn_snoise(float) — 1D simplex-style gradient noise (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_snoise(float, uint)`
// builtin, matching `src/builtins/lpfn/generative/snoise/snoise1_q32.rs`.
//
// LightPlayer's snoise family is a structural rewrite of simplex noise:
// gradient selection uses the integer lpfn_hash (noiz lineage, MIT) instead
// of the mod-289 float permute of the stegu/LYGIA original. Algorithm
// lineage: Stefan Gustavson & Ian McEwan's simplex noise (MIT,
// https://github.com/stegu/webgl-noise) via the noise-rs library.
// See docs/reports/2026-03-31-lpfx-license-audit.md.
//
// Depends on: hash.glsl

float lpfn_snoise(float x, uint seed) {
    int cell = int(floor(x));
    float dist = x - float(cell);

    // Hash cell coordinate to pick gradient (+1 or -1).
    uint h = lpfn_hash(uint(cell), seed);
    float gradient = ((h & 1u) == 0u) ? 1.0 : -1.0;

    float dotv = gradient * dist;

    // Quadratic support: t = 1 - dist^2, quintic falloff inside.
    float t = 1.0 - dist * dist;
    if (t > 0.0) {
        float t2 = t * t;
        float t3 = t2 * t;
        float t4 = t2 * t2;
        float t5 = t3 * t2;
        float falloff = 6.0 * t5 - 15.0 * t4 + 10.0 * t3;
        return dotv * falloff;
    }
    return 0.0;
}

// lpfn_snoise(vec2) — 2D simplex noise (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_snoise(vec2, uint)`
// builtin, matching `src/builtins/lpfn/generative/snoise/snoise2_q32.rs`.
//
// LightPlayer's snoise family is a structural rewrite of simplex noise:
// gradient selection uses the integer lpfn_hash (noiz lineage, MIT) + an
// 8-entry gradient LUT instead of the mod-289 float permute hashing of the
// stegu/LYGIA original. The skew/unskew simplex geometry and radial falloff
// follow Stefan Gustavson & Ian McEwan's simplex noise (MIT,
// https://github.com/stegu/webgl-noise) via the noise-rs library.
// See docs/reports/2026-03-31-lpfx-license-audit.md.
//
// Depends on: hash.glsl

// 8-entry gradient LUT: 4 axis-aligned + 4 diagonal (1/sqrt(2)).
vec2 lpfn_snoise2_grad(uint index) {
    float d = 0.7071067811865476; // 1/sqrt(2)
    uint i = index & 7u;
    if (i == 0u) { return vec2(1.0, 0.0); }
    if (i == 1u) { return vec2(-1.0, 0.0); }
    if (i == 2u) { return vec2(0.0, 1.0); }
    if (i == 3u) { return vec2(0.0, -1.0); }
    if (i == 4u) { return vec2(d, d); }
    if (i == 5u) { return vec2(-d, d); }
    if (i == 6u) { return vec2(d, -d); }
    return vec2(-d, -d);
}

// Surflet contribution: t = 1 - 2*|off|^2, falloff = 2t^2 + t^4.
float lpfn_snoise2_surflet(uint gi, vec2 off) {
    float t = 1.0 - 2.0 * dot(off, off);
    if (t > 0.0) {
        vec2 g = lpfn_snoise2_grad(gi);
        float t2 = t * t;
        float falloff = 2.0 * t2 + t2 * t2;
        return dot(g, off) * falloff;
    }
    return 0.0;
}

float lpfn_snoise(vec2 p, uint seed) {
    float skew = 0.36602540378443865;   // (sqrt(3) - 1) / 2
    float unskew = 0.21132486540518713; // (3 - sqrt(3)) / 6

    // Skew input space to determine the simplex cell.
    float s = (p.x + p.y) * skew;
    int cx = int(floor(p.x + s));
    int cy = int(floor(p.y + s));

    // Unskew the cell origin back to input space.
    float u = float(cx + cy) * unskew;
    vec2 origin = vec2(float(cx) - u, float(cy) - u);

    // Offsets from the three simplex corners.
    vec2 off1 = p - origin;
    // Middle corner: (1,0) if x-major, (0,1) if y-major (ties go x-major).
    float cmp = (off1.x >= off1.y) ? 1.0 : 0.0;
    vec2 order = vec2(cmp, 1.0 - cmp);
    vec2 off2 = off1 - order + unskew;
    vec2 off3 = off1 - 1.0 + 2.0 * unskew;

    // Gradient indices from the integer hash of each corner.
    uint gi0 = lpfn_hash(uvec2(uint(cx), uint(cy)), seed);
    uint gi1 = lpfn_hash(uvec2(uint(cx + int(order.x)), uint(cy + int(order.y))), seed);
    uint gi2 = lpfn_hash(uvec2(uint(cx + 1), uint(cy + 1)), seed);

    return lpfn_snoise2_surflet(gi0, off1)
        + lpfn_snoise2_surflet(gi1, off2)
        + lpfn_snoise2_surflet(gi2, off3);
}

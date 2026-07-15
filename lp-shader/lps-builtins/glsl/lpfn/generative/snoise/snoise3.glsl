// lpfn_snoise(vec3) — 3D simplex noise (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_snoise(vec3, uint)`
// builtin, matching `src/builtins/lpfn/generative/snoise/snoise3_q32.rs`.
//
// LightPlayer's snoise family is a structural rewrite of simplex noise:
// gradient selection uses the integer lpfn_hash (noiz lineage, MIT) + a
// 32-entry gradient LUT (12 edge gradients duplicated + 8 corner gradients)
// instead of the mod-289 float permute hashing of the stegu/LYGIA original.
// Simplex geometry and falloff follow Stefan Gustavson & Ian McEwan's
// simplex noise (MIT, https://github.com/stegu/webgl-noise) via noise-rs.
// See docs/reports/2026-03-31-lpfx-license-audit.md.
//
// Depends on: hash.glsl

// 32-entry gradient LUT: indices 0-11 edge gradients, 12-23 duplicates,
// 24-31 corner gradients.
vec3 lpfn_snoise3_grad(uint index) {
    float d = 0.7071067811865476;  // 1/sqrt(2)
    float e = 0.5773502691896258;  // 1/sqrt(3)
    uint i = index % 32u;
    if (i >= 12u && i < 24u) {
        i -= 12u; // duplicated edge gradients
    }
    if (i == 0u) { return vec3(d, d, 0.0); }
    if (i == 1u) { return vec3(-d, d, 0.0); }
    if (i == 2u) { return vec3(d, -d, 0.0); }
    if (i == 3u) { return vec3(-d, -d, 0.0); }
    if (i == 4u) { return vec3(d, 0.0, d); }
    if (i == 5u) { return vec3(-d, 0.0, d); }
    if (i == 6u) { return vec3(d, 0.0, -d); }
    if (i == 7u) { return vec3(-d, 0.0, -d); }
    if (i == 8u) { return vec3(0.0, d, d); }
    if (i == 9u) { return vec3(0.0, -d, d); }
    if (i == 10u) { return vec3(0.0, d, -d); }
    if (i == 11u) { return vec3(0.0, -d, -d); }
    if (i == 24u) { return vec3(e, e, e); }
    if (i == 25u) { return vec3(-e, e, e); }
    if (i == 26u) { return vec3(e, -e, e); }
    if (i == 27u) { return vec3(-e, -e, e); }
    if (i == 28u) { return vec3(e, e, -e); }
    if (i == 29u) { return vec3(-e, e, -e); }
    if (i == 30u) { return vec3(e, -e, -e); }
    return vec3(-e, -e, -e);
}

// Surflet contribution: t = 1 - 2*|off|^2, falloff = 2t^2 + t^4.
float lpfn_snoise3_surflet(uint gi, vec3 off) {
    float t = 1.0 - 2.0 * dot(off, off);
    if (t > 0.0) {
        vec3 g = lpfn_snoise3_grad(gi);
        float t2 = t * t;
        float falloff = 2.0 * t2 + t2 * t2;
        return dot(g, off) * falloff;
    }
    return 0.0;
}

float lpfn_snoise(vec3 p, uint seed) {
    float skew = 1.0 / 3.0;   // 3D skew factor
    float unskew = 1.0 / 6.0; // 3D unskew factor

    // Skew input space to determine the simplex cell.
    float s = (p.x + p.y + p.z) * skew;
    int cx = int(floor(p.x + s));
    int cy = int(floor(p.y + s));
    int cz = int(floor(p.z + s));

    // Unskew the cell origin back to input space.
    float u = float(cx + cy + cz) * unskew;
    vec3 origin = vec3(float(cx) - u, float(cy) - u, float(cz) - u);

    // Offsets from the four simplex corners.
    vec3 off1 = p - origin;

    // Rank-order the offsets to pick the traversal order (matches the
    // Rust branch structure exactly, including tie-breaking).
    vec3 order1 = vec3(0.0);
    vec3 order2 = vec3(0.0);
    if (off1.x >= off1.y) {
        if (off1.y >= off1.z) {
            order1 = vec3(1.0, 0.0, 0.0); // X Y Z
            order2 = vec3(1.0, 1.0, 0.0);
        } else if (off1.x >= off1.z) {
            order1 = vec3(1.0, 0.0, 0.0); // X Z Y
            order2 = vec3(1.0, 0.0, 1.0);
        } else {
            order1 = vec3(0.0, 0.0, 1.0); // Z X Y
            order2 = vec3(1.0, 0.0, 1.0);
        }
    } else {
        if (off1.y < off1.z) {
            order1 = vec3(0.0, 0.0, 1.0); // Z Y X
            order2 = vec3(0.0, 1.0, 1.0);
        } else if (off1.x < off1.z) {
            order1 = vec3(0.0, 1.0, 0.0); // Y Z X
            order2 = vec3(0.0, 1.0, 1.0);
        } else {
            order1 = vec3(0.0, 1.0, 0.0); // Y X Z
            order2 = vec3(1.0, 1.0, 0.0);
        }
    }

    vec3 off2 = off1 - order1 + unskew;
    vec3 off3 = off1 - order2 + 2.0 * unskew;
    vec3 off4 = off1 - 1.0 + 3.0 * unskew;

    // Gradient indices from the integer hash of each corner.
    uint gi0 = lpfn_hash(uvec3(uint(cx), uint(cy), uint(cz)), seed);
    uint gi1 = lpfn_hash(
        uvec3(uint(cx + int(order1.x)), uint(cy + int(order1.y)), uint(cz + int(order1.z))),
        seed);
    uint gi2 = lpfn_hash(
        uvec3(uint(cx + int(order2.x)), uint(cy + int(order2.y)), uint(cz + int(order2.z))),
        seed);
    uint gi3 = lpfn_hash(uvec3(uint(cx + 1), uint(cy + 1), uint(cz + 1)), seed);

    return lpfn_snoise3_surflet(gi0, off1)
        + lpfn_snoise3_surflet(gi1, off2)
        + lpfn_snoise3_surflet(gi2, off3)
        + lpfn_snoise3_surflet(gi3, off4);
}

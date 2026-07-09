// lpfn_psrdnoise(vec2) — 2D tiling simplex flow noise with rotating
// gradients and analytic derivative (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer
// `lpfn_psrdnoise(vec2, vec2, float, out vec2, uint)` builtin, matching
// `src/builtins/lpfn/generative/psrdnoise/psrdnoise2_q32.rs`.
//
// Original algorithm and implementation:
// Copyright 2021-2023 by Stefan Gustavson and Ian McEwan
// (https://github.com/stegu/psrdnoise), distributed by LYGIA in
// generative/psrdnoise.glsl. Published under the MIT license:
// https://opensource.org/license/mit/
// This LightPlayer port replaces the float mod-289 permute with an exact
// integer mod-289 permutation (same values where both are exact) and keeps
// the rest of the algorithm.
//
// The seed parameter is accepted but unused, matching the Rust
// implementation (psrdnoise derives its permutation from the lattice
// coordinates only).

// Integer corner hash; values stay in [0, 288].
int lpfn_psrdnoise2_hash(int iu, int iv) {
    int h = iu % 289;
    if (h < 0) { h += 289; }
    h = ((h * 51 + 2) * h + iv) % 289;
    if (h < 0) { h += 289; }
    h = ((h * 34 + 10) * h) % 289;
    if (h < 0) { h += 289; }
    return h;
}

float lpfn_psrdnoise(vec2 x, vec2 period, float alpha, out vec2 gradient, uint seed) {
    // Transform to simplex space (skewed grid).
    vec2 uv = vec2(x.x + x.y * 0.5, x.y);
    vec2 i0f = floor(uv);
    vec2 f0 = fract(uv);

    // cmp = step(f0.y, f0.x): 1 if f0.x >= f0.y else 0.
    float cmp = (f0.x >= f0.y) ? 1.0 : 0.0;
    vec2 o1 = vec2(cmp, 1.0 - cmp);

    vec2 i1f = i0f + o1;
    vec2 i2f = i0f + vec2(1.0, 1.0);

    // Transform corners back to input space.
    vec2 v0 = vec2(i0f.x - i0f.y * 0.5, i0f.y);
    vec2 v1 = vec2(v0.x + o1.x - o1.y * 0.5, v0.y + o1.y);
    vec2 v2 = vec2(v0.x + 0.5, v0.y + 1.0);

    vec2 x0 = x - v0;
    vec2 x1 = x - v1;
    vec2 x2 = x - v2;

    // Corner indices for hashing: wrapped when tiling, raw otherwise.
    int iu0 = 0;
    int iu1 = 0;
    int iu2 = 0;
    int iv0 = 0;
    int iv1 = 0;
    int iv2 = 0;
    if (period.x > 0.0 || period.y > 0.0) {
        vec3 xw = vec3(v0.x, v1.x, v2.x);
        vec3 yw = vec3(v0.y, v1.y, v2.y);
        if (period.x > 0.0) {
            xw = mod(xw, vec3(period.x));
        }
        if (period.y > 0.0) {
            yw = mod(yw, vec3(period.y));
        }
        iu0 = int(floor(xw.x + yw.x * 0.5 + 0.5));
        iu1 = int(floor(xw.y + yw.y * 0.5 + 0.5));
        iu2 = int(floor(xw.z + yw.z * 0.5 + 0.5));
        iv0 = int(floor(yw.x + 0.5));
        iv1 = int(floor(yw.y + 0.5));
        iv2 = int(floor(yw.z + 0.5));
    } else {
        iu0 = int(i0f.x);
        iu1 = int(i1f.x);
        iu2 = int(i2f.x);
        iv0 = int(i0f.y);
        iv1 = int(i1f.y);
        iv2 = int(i2f.y);
    }

    int hash0 = lpfn_psrdnoise2_hash(iu0, iv0);
    int hash1 = lpfn_psrdnoise2_hash(iu1, iv1);
    int hash2 = lpfn_psrdnoise2_hash(iu2, iv2);

    // Gradients: unit vectors at angle psi = hash * 0.07482 rotated by alpha.
    float psi0 = float(hash0) * 0.07482;
    float psi1 = float(hash1) * 0.07482;
    float psi2 = float(hash2) * 0.07482;
    vec2 g0 = vec2(cos(psi0 + alpha), sin(psi0 + alpha));
    vec2 g1 = vec2(cos(psi1 + alpha), sin(psi1 + alpha));
    vec2 g2 = vec2(cos(psi2 + alpha), sin(psi2 + alpha));

    // Radial decay: w = max(0.8 - |x_k|^2, 0).
    vec3 w = 0.8 - vec3(dot(x0, x0), dot(x1, x1), dot(x2, x2));
    w = max(w, vec3(0.0));

    vec3 w2 = w * w;
    vec3 w4 = w2 * w2;

    vec3 gdotx = vec3(dot(g0, x0), dot(g1, x1), dot(g2, x2));

    // Noise value: 10.9 * dot(w^4, g.x).
    float n = dot(w4, gdotx);

    // Analytic derivative.
    vec3 w3 = w2 * w;
    vec3 dw = -8.0 * w3 * gdotx;
    vec2 dn0 = g0 * w4.x + x0 * dw.x;
    vec2 dn1 = g1 * w4.y + x1 * dw.y;
    vec2 dn2 = g2 * w4.z + x2 * dw.z;
    gradient = 10.9 * (dn0 + dn1 + dn2);

    return 10.9 * n;
}

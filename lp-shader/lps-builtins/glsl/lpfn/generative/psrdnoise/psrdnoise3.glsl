// lpfn_psrdnoise(vec3) — 3D tiling simplex flow noise with rotating
// gradients and analytic derivative (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer
// `lpfn_psrdnoise(vec3, vec3, float, out vec3, uint)` builtin, matching
// `src/builtins/lpfn/generative/psrdnoise/psrdnoise3_q32.rs`.
//
// Original algorithm and implementation:
// Copyright 2021-2023 by Stefan Gustavson and Ian McEwan
// (https://github.com/stegu/psrdnoise), distributed by LYGIA in
// generative/psrdnoise.glsl. Published under the MIT license:
// https://opensource.org/license/mit/
// This LightPlayer port replaces the float mod-289 permute with an exact
// integer mod-289 permutation (same values where both are exact); gradients
// come from the Fibonacci-spiral sphere distribution of the original. The
// Q32 device implementation tabulates the spiral in a 289-entry LUT; the
// canonical computes it in closed form.
//
// The seed parameter is accepted but unused, matching the Rust
// implementation (psrdnoise derives its permutation from the lattice
// coordinates only).

// Integer corner hash; values stay in [0, 288].
// Matches the Rust port: permute(permute(permute(iw) + iv) + iu).
int lpfn_psrdnoise3_hash(int iu, int iv, int iw) {
    int h = iw % 289;
    if (h < 0) { h += 289; }
    h = ((h * 34 + 1) * h) % 289;
    if (h < 0) { h += 289; }
    int hv = (h + iv) % 289;
    if (hv < 0) { hv += 289; }
    h = (hv * 34 + 1) * h;
    h = (h + iu) % 289;
    if (h < 0) { h += 289; }
    h = ((h * 34 + 10) * h) % 289;
    if (h < 0) { h += 289; }
    return h;
}

// Gradient for a corner: Fibonacci-spiral sphere point, psi-rotated, then
// alpha-rotated about the tangent axis q.
vec3 lpfn_psrdnoise3_grad(int hash, float sinAlpha, float cosAlpha) {
    float h = float(hash);

    float theta = h * 3.883222077452858;   // 2*pi / golden ratio
    float sz = h * -0.006920415 + 0.996539792; // 1 - (2*h + 0.5)/289
    float psi = h * 0.108705628;           // 10*pi / 289

    float ct = cos(theta);
    float st = sin(theta);
    float szPrime = sqrt(1.0 - sz * sz);

    // Orthogonal tangent vector q and spiral point p.
    vec2 q = vec2(st, -ct);
    vec3 p = vec3(-sz * ct, -sz * st, szPrime);

    // Base gradient after psi rotation: g_b = cos(psi)*p + sin(psi)*(q, 0).
    vec3 gb = cos(psi) * p + sin(psi) * vec3(q, 0.0);

    // Alpha rotation about q (qz = 0).
    return vec3(
        cosAlpha * gb.x + sinAlpha * q.x,
        cosAlpha * gb.y + sinAlpha * q.y,
        cosAlpha * gb.z);
}

float lpfn_psrdnoise(vec3 x, vec3 period, float alpha, out vec3 gradient, uint seed) {
    float sinAlpha = sin(alpha);
    float cosAlpha = cos(alpha);

    // Transform to simplex space (tetrahedral grid):
    // uvw = x + dot(x, vec3(1/3)).
    float dotSum = (x.x + x.y + x.z) * (1.0 / 3.0);
    vec3 uvw = x + vec3(dotSum);

    vec3 i0 = floor(uvw);
    vec3 f0 = fract(uvw);

    // Rank-order u, v, w to find the simplex traversal order.
    // g_ = step(f0.xyx, f0.yzz): 1 if f0.xyx <= f0.yzz.
    vec3 g_ = step(f0.xyx, f0.yzz);
    vec3 l_ = 1.0 - g_;
    vec3 g = vec3(l_.z, g_.xy);
    vec3 l = vec3(l_.xy, g_.z);
    vec3 o1 = min(g, l);
    vec3 o2 = max(g, l);

    vec3 i1 = i0 + o1;
    vec3 i2 = i0 + o2;
    vec3 i3 = i0 + vec3(1.0);

    // Transform corners back to input space: v = i - dot(i, vec3(1/6)).
    float d0 = (i0.x + i0.y + i0.z) * (1.0 / 6.0);
    float d1 = (i1.x + i1.y + i1.z) * (1.0 / 6.0);
    float d2 = (i2.x + i2.y + i2.z) * (1.0 / 6.0);
    float d3 = (i3.x + i3.y + i3.z) * (1.0 / 6.0);
    vec3 v0 = i0 - vec3(d0);
    vec3 v1 = i1 - vec3(d1);
    vec3 v2 = i2 - vec3(d2);
    vec3 v3 = i3 - vec3(d3);

    int iu0 = 0;
    int iu1 = 0;
    int iu2 = 0;
    int iu3 = 0;
    int iv0 = 0;
    int iv1 = 0;
    int iv2 = 0;
    int iv3 = 0;
    int iw0 = 0;
    int iw1 = 0;
    int iw2 = 0;
    int iw3 = 0;
    vec3 x0 = vec3(0.0);
    vec3 x1 = vec3(0.0);
    vec3 x2 = vec3(0.0);
    vec3 x3 = vec3(0.0);

    if (period.x > 0.0 || period.y > 0.0 || period.z > 0.0) {
        // Wrap corner positions to the period per axis.
        vec4 vx = vec4(v0.x, v1.x, v2.x, v3.x);
        vec4 vy = vec4(v0.y, v1.y, v2.y, v3.y);
        vec4 vz = vec4(v0.z, v1.z, v2.z, v3.z);
        if (period.x > 0.0) {
            vx = mod(vx, vec4(period.x));
        }
        if (period.y > 0.0) {
            vy = mod(vy, vec4(period.y));
        }
        if (period.z > 0.0) {
            vz = mod(vz, vec4(period.z));
        }

        // Transform wrapped corners back to uvw and round to the lattice.
        vec4 dv = (vx + vy + vz) * (1.0 / 3.0);
        vec3 w0 = vec3(vx.x, vy.x, vz.x);
        vec3 w1 = vec3(vx.y, vy.y, vz.y);
        vec3 w2c = vec3(vx.z, vy.z, vz.z);
        vec3 w3c = vec3(vx.w, vy.w, vz.w);
        vec3 iw0v = floor(w0 + vec3(dv.x) + vec3(0.5));
        vec3 iw1v = floor(w1 + vec3(dv.y) + vec3(0.5));
        vec3 iw2v = floor(w2c + vec3(dv.z) + vec3(0.5));
        vec3 iw3v = floor(w3c + vec3(dv.w) + vec3(0.5));

        iu0 = int(iw0v.x); iv0 = int(iw0v.y); iw0 = int(iw0v.z);
        iu1 = int(iw1v.x); iv1 = int(iw1v.y); iw1 = int(iw1v.z);
        iu2 = int(iw2v.x); iv2 = int(iw2v.y); iw2 = int(iw2v.z);
        iu3 = int(iw3v.x); iv3 = int(iw3v.y); iw3 = int(iw3v.z);

        // Offsets from the (wrapped) corners.
        x0 = x - w0;
        x1 = x - w1;
        x2 = x - w2c;
        x3 = x - w3c;
    } else {
        iu0 = int(i0.x); iv0 = int(i0.y); iw0 = int(i0.z);
        iu1 = int(i1.x); iv1 = int(i1.y); iw1 = int(i1.z);
        iu2 = int(i2.x); iv2 = int(i2.y); iw2 = int(i2.z);
        iu3 = int(i3.x); iv3 = int(i3.y); iw3 = int(i3.z);

        x0 = x - v0;
        x1 = x - v1;
        x2 = x - v2;
        x3 = x - v3;
    }

    int hash0 = lpfn_psrdnoise3_hash(iu0, iv0, iw0);
    int hash1 = lpfn_psrdnoise3_hash(iu1, iv1, iw1);
    int hash2 = lpfn_psrdnoise3_hash(iu2, iv2, iw2);
    int hash3 = lpfn_psrdnoise3_hash(iu3, iv3, iw3);

    vec3 g0 = lpfn_psrdnoise3_grad(hash0, sinAlpha, cosAlpha);
    vec3 g1 = lpfn_psrdnoise3_grad(hash1, sinAlpha, cosAlpha);
    vec3 g2 = lpfn_psrdnoise3_grad(hash2, sinAlpha, cosAlpha);
    vec3 g3 = lpfn_psrdnoise3_grad(hash3, sinAlpha, cosAlpha);

    // Radial decay: w = max(0.5 - |x_k|^2, 0).
    vec4 w = 0.5 - vec4(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3));
    w = max(w, vec4(0.0));

    vec4 w2 = w * w;
    vec4 w3 = w2 * w;

    vec4 gdotx = vec4(dot(g0, x0), dot(g1, x1), dot(g2, x2), dot(g3, x3));

    // Noise value: 39.5 * dot(w^3, g.x).
    float n = dot(w3, gdotx);

    // Analytic derivative.
    vec4 dw = -6.0 * w2 * gdotx;
    vec3 dn0 = g0 * w3.x + x0 * dw.x;
    vec3 dn1 = g1 * w3.y + x1 * dw.y;
    vec3 dn2 = g2 * w3.z + x2 * dw.z;
    vec3 dn3 = g3 * w3.w + x3 * dw.w;
    gradient = 39.5 * (dn0 + dn1 + dn2 + dn3);

    return 39.5 * n;
}

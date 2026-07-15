// lpfn_worley(vec2) — 2D Worley (cellular) noise, distance variant
// (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_worley(vec2, uint)`
// builtin, matching `src/builtins/lpfn/generative/worley/worley2_q32.rs`.
//
// LightPlayer's worley is derived from the noise-rs library's
// range-function optimization (MIT/Apache-2.0,
// https://github.com/Razaekel/noise-rs) of Steven Worley's 1996 algorithm,
// using the integer lpfn_hash for feature points — NOT LYGIA's
// Prosperity-licensed worley.glsl
// (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Returns the squared euclidean distance to the nearest feature point,
// shifted to approximately [-1, 1].
//
// Depends on: hash.glsl

// Feature point for a cell: cell origin + hash-directed offset.
vec2 lpfn_worley2_point(uint index, int cellX, int cellY) {
    // length in [0, 0.5] from bits 3-7 of the hash.
    float lengthBits = float((index & 0xF8u) >> 3);
    float len = lengthBits * 0.5 / 31.0;
    float diag = len * 0.7071067811865476; // 1/sqrt(2)

    vec2 offset = vec2(0.0);
    uint dir = index & 0x07u;
    if (dir == 0u) { offset = vec2(diag, diag); }
    else if (dir == 1u) { offset = vec2(diag, -diag); }
    else if (dir == 2u) { offset = vec2(-diag, diag); }
    else if (dir == 3u) { offset = vec2(-diag, -diag); }
    else if (dir == 4u) { offset = vec2(len, 0.0); }
    else if (dir == 5u) { offset = vec2(-len, 0.0); }
    else if (dir == 6u) { offset = vec2(0.0, len); }
    else { offset = vec2(0.0, -len); }

    return vec2(float(cellX), float(cellY)) + offset;
}

float lpfn_worley(vec2 p, uint seed) {
    int cellX = int(floor(p.x));
    int cellY = int(floor(p.y));

    vec2 frac = p - vec2(float(cellX), float(cellY));

    // Near/far cells per axis based on which half of the cell we are in.
    int nearX = (frac.x > 0.5) ? cellX + 1 : cellX;
    int nearY = (frac.y > 0.5) ? cellY + 1 : cellY;
    int farX = (frac.x > 0.5) ? cellX : cellX + 1;
    int farY = (frac.y > 0.5) ? cellY : cellY + 1;

    // Feature point of the near cell.
    uint seedIndex = lpfn_hash(uvec2(uint(nearX), uint(nearY)), seed);
    vec2 seedPoint = lpfn_worley2_point(seedIndex, nearX, nearY);
    vec2 d = p - seedPoint;
    float dist = dot(d, d);

    // Range test values: squared distance to the cell midlines.
    float rangeX = (0.5 - frac.x) * (0.5 - frac.x);
    float rangeY = (0.5 - frac.y) * (0.5 - frac.y);

    if (rangeX < dist) {
        uint testIndex = lpfn_hash(uvec2(uint(farX), uint(nearY)), seed);
        vec2 tp = lpfn_worley2_point(testIndex, farX, nearY);
        vec2 td = p - tp;
        float testDist = dot(td, td);
        if (testDist < dist) {
            dist = testDist;
        }
    }

    if (rangeY < dist) {
        uint testIndex = lpfn_hash(uvec2(uint(nearX), uint(farY)), seed);
        vec2 tp = lpfn_worley2_point(testIndex, nearX, farY);
        vec2 td = p - tp;
        float testDist = dot(td, td);
        if (testDist < dist) {
            dist = testDist;
        }
    }

    if (rangeX < dist && rangeY < dist) {
        uint testIndex = lpfn_hash(uvec2(uint(farX), uint(farY)), seed);
        vec2 tp = lpfn_worley2_point(testIndex, farX, farY);
        vec2 td = p - tp;
        float testDist = dot(td, td);
        if (testDist < dist) {
            dist = testDist;
        }
    }

    // Map to approximately [-1, 1] (matches the Rust port's
    // (dist / 2) * 2 - 1 scaling).
    return (dist / 2.0) * 2.0 - 1.0;
}

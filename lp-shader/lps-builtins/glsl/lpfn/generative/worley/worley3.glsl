// lpfn_worley(vec3) — 3D Worley (cellular) noise, distance variant
// (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_worley(vec3, uint)`
// builtin, matching `src/builtins/lpfn/generative/worley/worley3_q32.rs`.
// noise-rs range-function lineage (MIT/Apache-2.0) with the integer
// lpfn_hash for feature points
// (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Returns the squared euclidean distance to the nearest feature point,
// scaled/shifted to approximately [-1, 1].
//
// Depends on: hash.glsl

// Feature point for a cell: cell origin + hash-directed offset.
vec3 lpfn_worley3_point(uint index, int cellX, int cellY, int cellZ) {
    // length in [0, 0.5] from bits 5-7 of the hash.
    float lengthBits = float((index & 0xE0u) >> 5);
    float len = lengthBits * 0.5 / 7.0;
    float diag = len * 0.7071067811865476; // 1/sqrt(2)

    vec3 offset = vec3(0.0);
    uint dir = index % 18u;
    if (dir == 0u) { offset = vec3(diag, diag, 0.0); }
    else if (dir == 1u) { offset = vec3(diag, -diag, 0.0); }
    else if (dir == 2u) { offset = vec3(-diag, diag, 0.0); }
    else if (dir == 3u) { offset = vec3(-diag, -diag, 0.0); }
    else if (dir == 4u) { offset = vec3(diag, 0.0, diag); }
    else if (dir == 5u) { offset = vec3(diag, 0.0, -diag); }
    else if (dir == 6u) { offset = vec3(-diag, 0.0, diag); }
    else if (dir == 7u) { offset = vec3(-diag, 0.0, -diag); }
    else if (dir == 8u) { offset = vec3(0.0, diag, diag); }
    else if (dir == 9u) { offset = vec3(0.0, diag, -diag); }
    else if (dir == 10u) { offset = vec3(0.0, -diag, diag); }
    else if (dir == 11u) { offset = vec3(0.0, -diag, -diag); }
    else if (dir == 12u) { offset = vec3(len, 0.0, 0.0); }
    else if (dir == 13u) { offset = vec3(0.0, len, 0.0); }
    else if (dir == 14u) { offset = vec3(0.0, 0.0, len); }
    else if (dir == 15u) { offset = vec3(-len, 0.0, 0.0); }
    else if (dir == 16u) { offset = vec3(0.0, -len, 0.0); }
    else { offset = vec3(0.0, 0.0, -len); }

    return vec3(float(cellX), float(cellY), float(cellZ)) + offset;
}

float lpfn_worley3_test(vec3 p, uint seed, int tx, int ty, int tz) {
    uint testIndex = lpfn_hash(uvec3(uint(tx), uint(ty), uint(tz)), seed);
    vec3 tp = lpfn_worley3_point(testIndex, tx, ty, tz);
    vec3 td = p - tp;
    return dot(td, td);
}

float lpfn_worley(vec3 p, uint seed) {
    int cellX = int(floor(p.x));
    int cellY = int(floor(p.y));
    int cellZ = int(floor(p.z));

    vec3 frac = p - vec3(float(cellX), float(cellY), float(cellZ));

    int nearX = (frac.x > 0.5) ? cellX + 1 : cellX;
    int nearY = (frac.y > 0.5) ? cellY + 1 : cellY;
    int nearZ = (frac.z > 0.5) ? cellZ + 1 : cellZ;
    int farX = (frac.x > 0.5) ? cellX : cellX + 1;
    int farY = (frac.y > 0.5) ? cellY : cellY + 1;
    int farZ = (frac.z > 0.5) ? cellZ : cellZ + 1;

    float dist = lpfn_worley3_test(p, seed, nearX, nearY, nearZ);

    float rangeX = (0.5 - frac.x) * (0.5 - frac.x);
    float rangeY = (0.5 - frac.y) * (0.5 - frac.y);
    float rangeZ = (0.5 - frac.z) * (0.5 - frac.z);

    // Single-axis checks.
    if (rangeX < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, farX, nearY, nearZ));
    }
    if (rangeY < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, nearX, farY, nearZ));
    }
    if (rangeZ < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, nearX, nearY, farZ));
    }

    // Two-axis checks.
    if (rangeX < dist && rangeY < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, farX, farY, nearZ));
    }
    if (rangeX < dist && rangeZ < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, farX, nearY, farZ));
    }
    if (rangeY < dist && rangeZ < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, nearX, farY, farZ));
    }

    // Three-axis check.
    if (rangeX < dist && rangeY < dist && rangeZ < dist) {
        dist = min(dist, lpfn_worley3_test(p, seed, farX, farY, farZ));
    }

    // Map to approximately [-1, 1] (matches the Rust port's
    // (dist / 3) * 2 - 1 scaling).
    return (dist / 3.0) * 2.0 - 1.0;
}

// lpfn_worley_value(vec3) — 3D Worley noise, value variant
// (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_worley_value(vec3, uint)`
// builtin, matching
// `src/builtins/lpfn/generative/worley/worley3_value_q32.rs`.
// Same nearest-feature search as lpfn_worley(vec3) (noise-rs lineage,
// MIT/Apache-2.0 — see docs/reports/2026-03-31-lpfx-license-audit.md), but
// returns a per-cell hash value in approximately [-1, 1].
//
// NOTE: value Worley is discontinuous at cell-ownership boundaries; the
// conformance harness allows a small fraction of boundary-flip outliers.
//
// Depends on: hash.glsl, generative/worley/worley3.glsl (feature-point helper)

float lpfn_worley_value(vec3 p, uint seed) {
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
    int seedCellX = nearX;
    int seedCellY = nearY;
    int seedCellZ = nearZ;

    float rangeX = (0.5 - frac.x) * (0.5 - frac.x);
    float rangeY = (0.5 - frac.y) * (0.5 - frac.y);
    float rangeZ = (0.5 - frac.z) * (0.5 - frac.z);

    // Single-axis checks.
    if (rangeX < dist) {
        float testDist = lpfn_worley3_test(p, seed, farX, nearY, nearZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = farX; seedCellY = nearY; seedCellZ = nearZ;
        }
    }
    if (rangeY < dist) {
        float testDist = lpfn_worley3_test(p, seed, nearX, farY, nearZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = nearX; seedCellY = farY; seedCellZ = nearZ;
        }
    }
    if (rangeZ < dist) {
        float testDist = lpfn_worley3_test(p, seed, nearX, nearY, farZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = nearX; seedCellY = nearY; seedCellZ = farZ;
        }
    }

    // Two-axis checks.
    if (rangeX < dist && rangeY < dist) {
        float testDist = lpfn_worley3_test(p, seed, farX, farY, nearZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = farX; seedCellY = farY; seedCellZ = nearZ;
        }
    }
    if (rangeX < dist && rangeZ < dist) {
        float testDist = lpfn_worley3_test(p, seed, farX, nearY, farZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = farX; seedCellY = nearY; seedCellZ = farZ;
        }
    }
    if (rangeY < dist && rangeZ < dist) {
        float testDist = lpfn_worley3_test(p, seed, nearX, farY, farZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = nearX; seedCellY = farY; seedCellZ = farZ;
        }
    }

    // Three-axis check.
    if (rangeX < dist && rangeY < dist && rangeZ < dist) {
        float testDist = lpfn_worley3_test(p, seed, farX, farY, farZ);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = farX; seedCellY = farY; seedCellZ = farZ;
        }
    }

    // Hash the owning cell, normalize low byte to [0, 1], map to [-1, 1].
    uint hashValue = lpfn_hash(uvec3(uint(seedCellX), uint(seedCellY), uint(seedCellZ)), seed);
    float normalized = float(hashValue & 0xFFu) / 255.0;
    return normalized * 2.0 - 1.0;
}

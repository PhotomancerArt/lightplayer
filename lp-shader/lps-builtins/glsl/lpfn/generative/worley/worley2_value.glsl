// lpfn_worley_value(vec2) — 2D Worley noise, value variant
// (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_worley_value(vec2, uint)`
// builtin, matching
// `src/builtins/lpfn/generative/worley/worley2_value_q32.rs`.
// Same nearest-feature search as lpfn_worley (noise-rs lineage,
// MIT/Apache-2.0 — see docs/reports/2026-03-31-lpfx-license-audit.md), but
// returns a per-cell hash value in approximately [-1, 1] instead of the
// distance.
//
// NOTE: value Worley is discontinuous at cell-ownership boundaries; the
// conformance harness allows a small fraction of boundary-flip outliers.
//
// Depends on: hash.glsl, generative/worley/worley2.glsl (feature-point helper)

float lpfn_worley_value(vec2 p, uint seed) {
    int cellX = int(floor(p.x));
    int cellY = int(floor(p.y));

    vec2 frac = p - vec2(float(cellX), float(cellY));

    int nearX = (frac.x > 0.5) ? cellX + 1 : cellX;
    int nearY = (frac.y > 0.5) ? cellY + 1 : cellY;
    int farX = (frac.x > 0.5) ? cellX : cellX + 1;
    int farY = (frac.y > 0.5) ? cellY : cellY + 1;

    uint seedIndex = lpfn_hash(uvec2(uint(nearX), uint(nearY)), seed);
    vec2 seedPoint = lpfn_worley2_point(seedIndex, nearX, nearY);
    vec2 d = p - seedPoint;
    float dist = dot(d, d);

    int seedCellX = nearX;
    int seedCellY = nearY;

    float rangeX = (0.5 - frac.x) * (0.5 - frac.x);
    float rangeY = (0.5 - frac.y) * (0.5 - frac.y);

    if (rangeX < dist) {
        uint testIndex = lpfn_hash(uvec2(uint(farX), uint(nearY)), seed);
        vec2 tp = lpfn_worley2_point(testIndex, farX, nearY);
        vec2 td = p - tp;
        float testDist = dot(td, td);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = farX;
            seedCellY = nearY;
        }
    }

    if (rangeY < dist) {
        uint testIndex = lpfn_hash(uvec2(uint(nearX), uint(farY)), seed);
        vec2 tp = lpfn_worley2_point(testIndex, nearX, farY);
        vec2 td = p - tp;
        float testDist = dot(td, td);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = nearX;
            seedCellY = farY;
        }
    }

    if (rangeX < dist && rangeY < dist) {
        uint testIndex = lpfn_hash(uvec2(uint(farX), uint(farY)), seed);
        vec2 tp = lpfn_worley2_point(testIndex, farX, farY);
        vec2 td = p - tp;
        float testDist = dot(td, td);
        if (testDist < dist) {
            dist = testDist;
            seedCellX = farX;
            seedCellY = farY;
        }
    }

    // Hash the owning cell, normalize low byte to [0, 1], map to [-1, 1].
    uint hashValue = lpfn_hash(uvec2(uint(seedCellX), uint(seedCellY)), seed);
    float normalized = float(hashValue & 0xFFu) / 255.0;
    return normalized * 2.0 - 1.0;
}

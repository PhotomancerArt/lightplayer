// test run
//
// Regression: WASM vec2(scalar)/vec3(scalar) broadcast must leave exactly N stack
// slots before an import call. An off-by-one there shifted lpfn_psrdnoise arguments
// (e.g. vec2(0.0) period) and made noise independent of the first coordinate.

// ============================================================================
// lpfn_psrdnoise() 2D — period via vec2(scalar) broadcast
// ============================================================================

float test_lpfn_psrdnoise2_basic_range() {
    vec2 g;
    float n = lpfn_psrdnoise(vec2(0.5, 0.25), vec2(0.0, 0.0), 0.0, g, 0u);
    return (n >= -2.0 && n <= 2.0) ? 1.0 : 0.0;
}

// run: test_lpfn_psrdnoise2_basic_range() ~= 1.0

float test_lpfn_psrdnoise2_broadcast_period_zero_x_varies() {
    vec2 g1;
    vec2 g2;
    // Same y, alpha, seed; period is broadcast vec2(0.0) (must not shift stack).
    // Use a large Δx so Q32 noise differs reliably (small steps can tie in fixed-point).
    float n1 = lpfn_psrdnoise(vec2(0.1, 1.7), vec2(0.0), 0.5, g1, 0u);
    float n2 = lpfn_psrdnoise(vec2(14.2, 1.7), vec2(0.0), 0.5, g2, 0u);
    return abs(n1 - n2) > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfn_psrdnoise2_broadcast_period_zero_x_varies() ~= 1.0

float test_lpfn_psrdnoise2_broadcast_period_matches_explicit() {
    vec2 ga;
    vec2 gb;
    float a = lpfn_psrdnoise(vec2(1.2, 2.3), vec2(2.0), 0.25, ga, 0u);
    float b = lpfn_psrdnoise(vec2(1.2, 2.3), vec2(2.0, 2.0), 0.25, gb, 0u);
    bool ok = abs(a - b) < 0.0001 &&
              abs(ga.x - gb.x) < 0.0001 &&
              abs(ga.y - gb.y) < 0.0001;
    return ok ? 1.0 : 0.0;
}

// run: test_lpfn_psrdnoise2_broadcast_period_matches_explicit() ~= 1.0

// ============================================================================
// lpfn_psrdnoise() 3D — vec3(scalar) broadcast for period
// ============================================================================

float test_lpfn_psrdnoise3_broadcast_period_zero_x_varies() {
    vec3 g1;
    vec3 g2;
    float n1 = lpfn_psrdnoise(vec3(0.1, 1.0, 2.0), vec3(0.0), 0.5, g1, 0u);
    float n2 = lpfn_psrdnoise(vec3(2.1, 1.0, 2.0), vec3(0.0), 0.5, g2, 0u);
    return abs(n1 - n2) > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfn_psrdnoise3_broadcast_period_zero_x_varies() ~= 1.0

float test_lpfn_psrdnoise3_broadcast_period_matches_explicit() {
    vec3 ga;
    vec3 gb;
    float a = lpfn_psrdnoise(vec3(0.2, 0.3, 0.4), vec3(1.5), 0.1, ga, 0u);
    float b = lpfn_psrdnoise(vec3(0.2, 0.3, 0.4), vec3(1.5, 1.5, 1.5), 0.1, gb, 0u);
    bool ok = abs(a - b) < 0.0001 &&
              abs(ga.x - gb.x) < 0.0001 &&
              abs(ga.y - gb.y) < 0.0001 &&
              abs(ga.z - gb.z) < 0.0001;
    return ok ? 1.0 : 0.0;
}

// run: test_lpfn_psrdnoise3_broadcast_period_matches_explicit() ~= 1.0

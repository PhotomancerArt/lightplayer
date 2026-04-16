// test run

// ============================================================================
// lpfn_gnoise(): Gradient noise functions
// ============================================================================

float test_lpfn_gnoise_1d() {
    // Test 1D gradient noise - should be in reasonable range
    float x = 0.5;
    uint seed = 0u;
    float n = lpfn_gnoise(x, seed);
    return (n >= -2.0 && n <= 2.0) ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_1d() == 1.0

float test_lpfn_gnoise_2d() {
    // Test 2D gradient noise - should be in reasonable range
    vec2 p = vec2(0.5, 0.5);
    uint seed = 0u;
    float n = lpfn_gnoise(p, seed);
    return (n >= -2.0 && n <= 2.0) ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_2d() == 1.0

float test_lpfn_gnoise_3d() {
    // Test 3D gradient noise - should be in reasonable range
    vec3 p = vec3(0.5, 0.5, 0.5);
    uint seed = 0u;
    float n = lpfn_gnoise(p, seed);
    return (n >= -2.0 && n <= 2.0) ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_3d() == 1.0

float test_lpfn_gnoise_3d_tile() {
    // Test 3D gradient noise with tiling - should be in reasonable range
    vec3 p = vec3(0.5, 0.5, 0.5);
    float tileLength = 4.0;
    uint seed = 0u;
    float n = lpfn_gnoise(p, tileLength, seed);
    return (n >= -2.0 && n <= 2.0) ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_3d_tile() == 1.0

float test_lpfn_gnoise_deterministic() {
    // Same inputs should produce same output
    float n1 = lpfn_gnoise(0.5, 0u);
    float n2 = lpfn_gnoise(0.5, 0u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_deterministic() == 1.0

float test_lpfn_gnoise_different_seeds() {
    // Different seeds should produce different outputs
    float diff1 = abs(lpfn_gnoise(0.5, 0u) - lpfn_gnoise(0.5, 1u));
    float diff2 = abs(lpfn_gnoise(1.5, 0u) - lpfn_gnoise(1.5, 1u));
    bool has_diff = diff1 > 0.01 || diff2 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_different_seeds() == 1.0

float test_lpfn_gnoise_different_positions() {
    // Different positions should produce different outputs (with high probability)
    float n1 = lpfn_gnoise(0.0, 0u);
    float n2 = lpfn_gnoise(1.0, 0u);
    return abs(n1 - n2) > 0.01 ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_different_positions() == 1.0

float test_lpfn_gnoise_smoothness() {
    // Gradient noise should be relatively smooth (small changes in input produce small changes in output)
    float n1 = lpfn_gnoise(0.5, 0u);
    float n2 = lpfn_gnoise(0.51, 0u);
    float diff = abs(n1 - n2);
    // Should be relatively small (not huge jumps)
    return diff < 1.0 ? 1.0 : 0.0;
}

// run: test_lpfn_gnoise_smoothness() == 1.0

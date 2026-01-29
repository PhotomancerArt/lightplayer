// test run
// target riscv32.q32

// ============================================================================
// lpfx_random(): Random noise functions
// ============================================================================

float test_lpfx_random_1d() {
    // Test 1D random - should be in [0, 1] range
    float x = 0.5;
    uint seed = 0u;
    float n = lpfx_random(x, seed);
    return (n >= 0.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_random_1d() == 1.0

float test_lpfx_random_2d() {
    // Test 2D random - should be in [0, 1] range
    vec2 p = vec2(0.5, 0.5);
    uint seed = 0u;
    float n = lpfx_random(p, seed);
    return (n >= 0.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_random_2d() == 1.0

float test_lpfx_random_3d() {
    // Test 3D random - should be in [0, 1] range
    vec3 p = vec3(0.5, 0.5, 0.5);
    uint seed = 0u;
    float n = lpfx_random(p, seed);
    return (n >= 0.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_random_3d() == 1.0

float test_lpfx_random_deterministic() {
    // Same inputs should produce same output
    float n1 = lpfx_random(0.5, 0u);
    float n2 = lpfx_random(0.5, 0u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfx_random_deterministic() == 1.0

float test_lpfx_random_different_seeds() {
    // Different seeds should produce different outputs
    float diff1 = abs(lpfx_random(0.5, 0u) - lpfx_random(0.5, 1u));
    float diff2 = abs(lpfx_random(1.5, 0u) - lpfx_random(1.5, 1u));
    bool has_diff = diff1 > 0.01 || diff2 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_random_different_seeds() == 1.0

float test_lpfx_random_different_positions() {
    // Different positions should produce different outputs (with high probability)
    float n1 = lpfx_random(0.0, 0u);
    float n2 = lpfx_random(1.0, 0u);
    return abs(n1 - n2) > 0.01 ? 1.0 : 0.0;
}

// run: test_lpfx_random_different_positions() == 1.0

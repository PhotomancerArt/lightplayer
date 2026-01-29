// test run
// target riscv32.q32

// ============================================================================
// lpfx_fbm(): Fractional Brownian Motion noise functions
// ============================================================================

float test_lpfx_fbm_2d() {
    // Test 2D FBM - should be in reasonable range
    vec2 p = vec2(0.5, 0.5);
    int octaves = 4;
    uint seed = 0u;
    float n = lpfx_fbm(p, octaves, seed);
    return (n >= -5.0 && n <= 5.0) ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_2d() == 1.0

float test_lpfx_fbm_3d() {
    // Test 3D FBM - should be in reasonable range
    vec3 p = vec3(0.5, 0.5, 0.5);
    int octaves = 4;
    uint seed = 0u;
    float n = lpfx_fbm(p, octaves, seed);
    return (n >= -5.0 && n <= 5.0) ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_3d() == 1.0

float test_lpfx_fbm_3d_tile() {
    // Test 3D FBM with tiling - should be in reasonable range
    vec3 p = vec3(0.5, 0.5, 0.5);
    float tileLength = 4.0;
    int octaves = 4;
    uint seed = 0u;
    float n = lpfx_fbm(p, tileLength, octaves, seed);
    return (n >= -5.0 && n <= 5.0) ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_3d_tile() == 1.0

float test_lpfx_fbm_deterministic() {
    // Same inputs should produce same output
    float n1 = lpfx_fbm(vec2(0.5, 0.5), 4, 0u);
    float n2 = lpfx_fbm(vec2(0.5, 0.5), 4, 0u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_deterministic() == 1.0

float test_lpfx_fbm_different_seeds() {
    // Different seeds should produce different outputs
    float diff1 = abs(lpfx_fbm(vec2(0.5, 0.5), 4, 0u) - lpfx_fbm(vec2(0.5, 0.5), 4, 1u));
    float diff2 = abs(lpfx_fbm(vec2(1.5, 1.5), 4, 0u) - lpfx_fbm(vec2(1.5, 1.5), 4, 1u));
    bool has_diff = diff1 > 0.01 || diff2 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_different_seeds() == 1.0

float test_lpfx_fbm_octaves_effect() {
    // More octaves should produce different (more detailed) output
    float n1 = lpfx_fbm(vec2(0.5, 0.5), 1, 0u);
    float n2 = lpfx_fbm(vec2(0.5, 0.5), 4, 0u);
    // They might be similar but should be different in general
    float diff = abs(n1 - n2);
    // At least some difference expected (not always identical)
    return diff > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_octaves_effect() == 1.0

float test_lpfx_fbm_smoothness() {
    // FBM should be relatively smooth (small changes in input produce small changes in output)
    float n1 = lpfx_fbm(vec2(0.5, 0.5), 4, 0u);
    float n2 = lpfx_fbm(vec2(0.51, 0.5), 4, 0u);
    float diff = abs(n1 - n2);
    // Should be relatively small (not huge jumps)
    return diff < 2.0 ? 1.0 : 0.0;
}

// run: test_lpfx_fbm_smoothness() == 1.0

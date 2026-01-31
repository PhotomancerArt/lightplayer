// test run
// target riscv32.q32

// ============================================================================
// lpfx_snoise(): 3D Simplex noise function
// ============================================================================

float test_lpfx_snoise3_basic() {
    // Test basic 3D simplex noise - should be in [-1, 1] range
    vec3 p = vec3(0.5, 0.5, 0.5);
    uint seed = 0u;
    float n = lpfx_snoise(p, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_snoise3_basic() == 1.0

float test_lpfx_snoise3_origin() {
    // Test at origin - should be in [-1, 1] range
    vec3 p = vec3(0.0, 0.0, 0.0);
    uint seed = 0u;
    float n = lpfx_snoise(p, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_snoise3_origin() == 1.0

float test_lpfx_snoise3_deterministic() {
    // Same inputs should produce same output
    vec3 p = vec3(0.5, 0.5, 0.5);
    float n1 = lpfx_snoise(p, 0u);
    float n2 = lpfx_snoise(p, 0u);
    return abs(n1 - n2);
}

// run: test_lpfx_snoise3_deterministic() ~= 0.0

float test_lpfx_snoise3_different_seeds() {
    // Different seeds should produce different outputs
    // Test many points with varied coordinates - seeds should produce different outputs at some points
    float diff1 = abs(lpfx_snoise(vec3(0.5, 0.5, 0.5), 0u) - lpfx_snoise(vec3(0.5, 0.5, 0.5), 1u));
    float diff2 = abs(lpfx_snoise(vec3(1.2, 1.3, 1.4), 0u) - lpfx_snoise(vec3(1.2, 1.3, 1.4), 1u));
    float diff3 = abs(lpfx_snoise(vec3(2.1, 2.2, 2.3), 0u) - lpfx_snoise(vec3(2.1, 2.2, 2.3), 1u));
    float diff4 = abs(lpfx_snoise(vec3(3.7, 3.8, 3.9), 0u) - lpfx_snoise(vec3(3.7, 3.8, 3.9), 1u));
    float diff5 = abs(lpfx_snoise(vec3(4.3, 4.4, 4.5), 0u) - lpfx_snoise(vec3(4.3, 4.4, 4.5), 1u));
    float diff6 = abs(lpfx_snoise(vec3(0.1, 0.2, 0.3), 0u) - lpfx_snoise(vec3(0.1, 0.2, 0.3), 1u));
    float diff7 = abs(lpfx_snoise(vec3(5.5, 5.6, 5.7), 0u) - lpfx_snoise(vec3(5.5, 5.6, 5.7), 1u));
    float diff8 = abs(lpfx_snoise(vec3(6.1, 6.2, 6.3), 0u) - lpfx_snoise(vec3(6.1, 6.2, 6.3), 1u));
    float diff9 = abs(lpfx_snoise(vec3(7.4, 7.5, 7.6), 0u) - lpfx_snoise(vec3(7.4, 7.5, 7.6), 1u));
    float diff10 = abs(lpfx_snoise(vec3(8.8, 8.9, 9.0), 0u) - lpfx_snoise(vec3(8.8, 8.9, 9.0), 1u));
    
    bool has_diff = diff1 > 0.01 || diff2 > 0.01 || diff3 > 0.01 || diff4 > 0.01 || diff5 > 0.01 ||
                    diff6 > 0.01 || diff7 > 0.01 || diff8 > 0.01 || diff9 > 0.01 || diff10 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_snoise3_different_seeds() == 1.0

float test_lpfx_snoise3_different_positions() {
    // Different positions should produce different outputs (with high probability)
    // Test multiple position pairs to ensure we find differences
    float diff1 = abs(lpfx_snoise(vec3(0.0, 0.0, 0.0), 0u) - lpfx_snoise(vec3(1.0, 1.0, 1.0), 0u));
    float diff2 = abs(lpfx_snoise(vec3(0.5, 0.5, 0.5), 0u) - lpfx_snoise(vec3(2.5, 2.5, 2.5), 0u));
    float diff3 = abs(lpfx_snoise(vec3(1.0, 0.0, 0.0), 0u) - lpfx_snoise(vec3(0.0, 1.0, 0.0), 0u));
    float diff4 = abs(lpfx_snoise(vec3(2.0, 2.0, 2.0), 0u) - lpfx_snoise(vec3(3.0, 3.0, 3.0), 0u));
    float diff5 = abs(lpfx_snoise(vec3(0.0, 0.0, 1.0), 0u) - lpfx_snoise(vec3(1.0, 1.0, 0.0), 0u));
    
    bool has_diff = diff1 > 0.01 || diff2 > 0.01 || diff3 > 0.01 || diff4 > 0.01 || diff5 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_snoise3_different_positions() == 1.0

float test_lpfx_snoise3_range() {
    // Test multiple values to ensure they're in valid range
    float n1 = lpfx_snoise(vec3(0.0, 0.0, 0.0), 0u);
    float n2 = lpfx_snoise(vec3(1.0, 0.0, 0.0), 0u);
    float n3 = lpfx_snoise(vec3(0.0, 1.0, 0.0), 0u);
    float n4 = lpfx_snoise(vec3(0.0, 0.0, 1.0), 0u);
    float n5 = lpfx_snoise(vec3(10.0, 10.0, 10.0), 0u);
    
    // All should be in [-1, 1] range
    bool valid = n1 >= -1.0 && n1 <= 1.0 &&
                 n2 >= -1.0 && n2 <= 1.0 &&
                 n3 >= -1.0 && n3 <= 1.0 &&
                 n4 >= -1.0 && n4 <= 1.0 &&
                 n5 >= -1.0 && n5 <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_snoise3_range() == 1.0

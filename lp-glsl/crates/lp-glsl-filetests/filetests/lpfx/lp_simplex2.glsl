// test run
// target riscv32.q32

// ============================================================================
// lpfx_snoise(): 2D Simplex noise function
// ============================================================================

float test_lpfx_snoise2_basic() {
    // Test basic 2D simplex noise - should be in [-1, 1] range
    vec2 p = vec2(0.5, 0.5);
    uint seed = 0u;
    float n = lpfx_snoise(p, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_snoise2_basic() == 1.0

float test_lpfx_snoise2_origin() {
    // Test at origin - should be in [-1, 1] range
    vec2 p = vec2(0.0, 0.0);
    uint seed = 0u;
    float n = lpfx_snoise(p, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_snoise2_origin() == 1.0

float test_lpfx_snoise2_deterministic() {
    // Same inputs should produce same output
    vec2 p = vec2(0.5, 0.5);
    float n1 = lpfx_snoise(p, 0u);
    float n2 = lpfx_snoise(p, 0u);
    return abs(n1 - n2);
}

// run: test_lpfx_snoise2_deterministic() ~= 0.0

float test_lpfx_snoise2_different_seeds() {
    // Different seeds should produce different outputs
    // Test multiple points - seeds should produce different outputs at some points
    float diff1 = abs(lpfx_snoise(vec2(0.5, 0.5), 0u) - lpfx_snoise(vec2(0.5, 0.5), 1u));
    float diff2 = abs(lpfx_snoise(vec2(1.5, 1.5), 0u) - lpfx_snoise(vec2(1.5, 1.5), 1u));
    float diff3 = abs(lpfx_snoise(vec2(2.5, 2.5), 0u) - lpfx_snoise(vec2(2.5, 2.5), 1u));
    float diff4 = abs(lpfx_snoise(vec2(3.5, 3.5), 0u) - lpfx_snoise(vec2(3.5, 3.5), 1u));
    float diff5 = abs(lpfx_snoise(vec2(4.5, 4.5), 0u) - lpfx_snoise(vec2(4.5, 4.5), 1u));
    
    bool has_diff = diff1 > 0.01 || diff2 > 0.01 || diff3 > 0.01 || diff4 > 0.01 || diff5 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_snoise2_different_seeds() == 1.0

float test_lpfx_snoise2_different_positions() {
    // Different positions should produce different outputs (with high probability)
    float n1 = lpfx_snoise(vec2(0.0, 0.0), 0u);
    float n2 = lpfx_snoise(vec2(1.0, 1.0), 0u);
    return abs(n1 - n2) > 0.01 ? 1.0 : 0.0;
}

// run: test_lpfx_snoise2_different_positions() == 1.0

float test_lpfx_snoise2_range() {
    // Test multiple values to ensure they're in valid range
    float n1 = lpfx_snoise(vec2(0.0, 0.0), 0u);
    float n2 = lpfx_snoise(vec2(1.0, 0.0), 0u);
    float n3 = lpfx_snoise(vec2(0.0, 1.0), 0u);
    float n4 = lpfx_snoise(vec2(10.0, 10.0), 0u);
    
    // All should be in [-1, 1] range
    bool valid = n1 >= -1.0 && n1 <= 1.0 &&
                 n2 >= -1.0 && n2 <= 1.0 &&
                 n3 >= -1.0 && n3 <= 1.0 &&
                 n4 >= -1.0 && n4 <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_snoise2_range() == 1.0

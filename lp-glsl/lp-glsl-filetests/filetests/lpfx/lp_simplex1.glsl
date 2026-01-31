// test run
// target riscv32.q32

// ============================================================================
// lpfx_snoise(): 1D Simplex noise function
// ============================================================================

float test_lpfx_snoise1_basic() {
    // Test basic 1D simplex noise - should be in [-1, 1] range
    float x = 0.5;
    uint seed = 0u;
    float n = lpfx_snoise(x, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_snoise1_basic() == 1.0

float test_lpfx_snoise1_zero() {
    // Test at origin - should be in [-1, 1] range
    float x = 0.0;
    uint seed = 0u;
    float n = lpfx_snoise(x, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_snoise1_zero() == 1.0

float test_lpfx_snoise1_deterministic() {
    // Same inputs should produce same output
    float n1 = lpfx_snoise(0.5, 0u);
    float n2 = lpfx_snoise(0.5, 0u);
    return abs(n1 - n2);
}

// run: test_lpfx_snoise1_deterministic() ~= 0.0

float test_lpfx_snoise1_different_seeds() {
    // Different seeds should produce different outputs (at least sometimes)
    // Test multiple points because seeds don't guarantee different outputs at every point
    // (if both seeds produce gradients with the same sign, outputs will be the same)
    float diff1 = abs(lpfx_snoise(0.5, 0u) - lpfx_snoise(0.5, 1u));
    float diff2 = abs(lpfx_snoise(1.5, 0u) - lpfx_snoise(1.5, 1u));
    float diff3 = abs(lpfx_snoise(2.5, 0u) - lpfx_snoise(2.5, 1u));
    float diff4 = abs(lpfx_snoise(3.5, 0u) - lpfx_snoise(3.5, 1u));
    float diff5 = abs(lpfx_snoise(4.5, 0u) - lpfx_snoise(4.5, 1u));
    
    // At least one should be different
    bool has_diff = diff1 > 0.01 || diff2 > 0.01 || diff3 > 0.01 || diff4 > 0.01 || diff5 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_snoise1_different_seeds() == 1.0

float test_lpfx_snoise1_range() {
    // Test multiple values to ensure they're in valid range
    float n1 = lpfx_snoise(0.0, 0u);
    float n2 = lpfx_snoise(1.0, 0u);
    float n3 = lpfx_snoise(2.0, 0u);
    float n4 = lpfx_snoise(10.0, 0u);
    
    // All should be in [-1, 1] range
    bool valid = n1 >= -1.0 && n1 <= 1.0 &&
                 n2 >= -1.0 && n2 <= 1.0 &&
                 n3 >= -1.0 && n3 <= 1.0 &&
                 n4 >= -1.0 && n4 <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_snoise1_range() == 1.0

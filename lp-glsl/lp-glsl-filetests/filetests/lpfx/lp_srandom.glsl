// test run
// target riscv32.q32

// ============================================================================
// lpfx_srandom(): Signed random noise functions
// ============================================================================

float test_lpfx_srandom_1d() {
    // Test 1D signed random - should be in [-1, 1] range
    float x = 0.5;
    uint seed = 0u;
    float n = lpfx_srandom(x, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_1d() == 1.0

float test_lpfx_srandom_2d() {
    // Test 2D signed random - should be in [-1, 1] range
    vec2 p = vec2(0.5, 0.5);
    uint seed = 0u;
    float n = lpfx_srandom(p, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_2d() == 1.0

float test_lpfx_srandom_3d() {
    // Test 3D signed random - should be in [-1, 1] range
    vec3 p = vec3(0.5, 0.5, 0.5);
    uint seed = 0u;
    float n = lpfx_srandom(p, seed);
    return (n >= -1.0 && n <= 1.0) ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_3d() == 1.0

float test_lpfx_srandom_3d_vec() {
    // Test 3D signed random vector - should be in [-1, 1] range for each component
    vec3 p = vec3(0.5, 0.5, 0.5);
    uint seed = 0u;
    vec3 n = lpfx_srandom3_vec(p, seed);
    bool valid = n.x >= -1.0 && n.x <= 1.0 &&
                 n.y >= -1.0 && n.y <= 1.0 &&
                 n.z >= -1.0 && n.z <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_3d_vec() == 1.0

float test_lpfx_srandom_3d_tile() {
    // Test 3D signed random with tiling - should be in [-1, 1] range
    vec3 p = vec3(0.5, 0.5, 0.5);
    float tileLength = 4.0;
    uint seed = 0u;
    vec3 n = lpfx_srandom3_tile(p, tileLength, seed);
    bool valid = n.x >= -1.0 && n.x <= 1.0 &&
                 n.y >= -1.0 && n.y <= 1.0 &&
                 n.z >= -1.0 && n.z <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_3d_tile() == 1.0

float test_lpfx_srandom_deterministic() {
    // Same inputs should produce same output
    float n1 = lpfx_srandom(0.5, 0u);
    float n2 = lpfx_srandom(0.5, 0u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_deterministic() == 1.0

float test_lpfx_srandom_different_seeds() {
    // Different seeds should produce different outputs
    float diff1 = abs(lpfx_srandom(0.5, 0u) - lpfx_srandom(0.5, 1u));
    float diff2 = abs(lpfx_srandom(1.5, 0u) - lpfx_srandom(1.5, 1u));
    bool has_diff = diff1 > 0.01 || diff2 > 0.01;
    return has_diff ? 1.0 : 0.0;
}

// run: test_lpfx_srandom_different_seeds() == 1.0

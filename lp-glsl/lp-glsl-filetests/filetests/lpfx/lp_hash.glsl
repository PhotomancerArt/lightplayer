// test run
// target riscv32.q32

// ============================================================================
// lpfx_hash(): Hash function for noise generation
// ============================================================================

uint test_lpfx_hash_1d() {
    // Test 1D hash function - should return non-zero
    uint x = 42u;
    uint seed = 123u;
    uint h = lpfx_hash(x, seed);
    return h != 0u ? 1u : 0u;
}

// run: test_lpfx_hash_1d() == 1u

uint test_lpfx_hash_2d() {
    // Test 2D hash function - should return non-zero
    uvec2 xy = uvec2(42u, 100u);
    uint seed = 123u;
    uint h = lpfx_hash(xy, seed);
    return h != 0u ? 1u : 0u;
}

// run: test_lpfx_hash_2d() == 1u

uint test_lpfx_hash_3d() {
    // Test 3D hash function - should return non-zero
    uvec3 xyz = uvec3(42u, 100u, 200u);
    uint seed = 123u;
    uint h = lpfx_hash(xyz, seed);
    return h != 0u ? 1u : 0u;
}

// run: test_lpfx_hash_3d() == 1u

uint test_lpfx_hash_deterministic() {
    // Same inputs should produce same output
    uint h1 = lpfx_hash(42u, 123u);
    uint h2 = lpfx_hash(42u, 123u);
    return h1 == h2 ? 1u : 0u;
}

// run: test_lpfx_hash_deterministic() == 1u

uint test_lpfx_hash_different_inputs() {
    // Different inputs should produce different outputs (with high probability)
    uint h1 = lpfx_hash(42u, 123u);
    uint h2 = lpfx_hash(43u, 123u);
    return h1 != h2 ? 1u : 0u;
}

// run: test_lpfx_hash_different_inputs() == 1u

uint test_lpfx_hash_different_seeds() {
    // Different seeds should produce different outputs
    uint h1 = lpfx_hash(42u, 123u);
    uint h2 = lpfx_hash(42u, 124u);
    return h1 != h2 ? 1u : 0u;
}

// run: test_lpfx_hash_different_seeds() == 1u

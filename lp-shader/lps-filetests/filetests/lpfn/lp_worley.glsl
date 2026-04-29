// test run

// ============================================================================
// lpfn_worley() - Worley/cellular noise functions
// ============================================================================

float test_lpfn_worley2_basic_range() {
    // Test 2D worley - should be in approximately [-1, 1] range
    float n = lpfn_worley(vec2(5.0, 3.0), 123u);
    return (n >= -1.5 && n <= 1.5) ? 1.0 : 0.0;
}

// run: test_lpfn_worley2_basic_range() == 1.0

float test_lpfn_worley3_basic_range() {
    // Test 3D worley - should be in approximately [-1, 1] range
    float n = lpfn_worley(vec3(1.0, 2.0, 3.0), 456u);
    return (n >= -1.5 && n <= 1.5) ? 1.0 : 0.0;
}

// run: test_lpfn_worley3_basic_range() == 1.0

float test_lpfn_worley2_determinism() {
    // Same inputs should produce same output
    float n1 = lpfn_worley(vec2(1.0, 1.0), 999u);
    float n2 = lpfn_worley(vec2(1.0, 1.0), 999u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley2_determinism() == 1.0

float test_lpfn_worley3_determinism() {
    // Same inputs should produce same output
    float n1 = lpfn_worley(vec3(1.0, 2.0, 3.0), 789u);
    float n2 = lpfn_worley(vec3(1.0, 2.0, 3.0), 789u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley3_determinism() == 1.0

float test_lpfn_worley2_different_inputs() {
    // Different positions should produce different outputs
    float n1 = lpfn_worley(vec2(0.0, 0.0), 0u);
    float n2 = lpfn_worley(vec2(5.0, 5.0), 0u);
    return abs(n1 - n2) > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley2_different_inputs() == 1.0

float test_lpfn_worley3_different_inputs() {
    // Different positions should produce different outputs
    float n1 = lpfn_worley(vec3(0.0, 0.0, 0.0), 0u);
    float n2 = lpfn_worley(vec3(3.0, 3.0, 3.0), 0u);
    return abs(n1 - n2) > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley3_different_inputs() == 1.0

float test_lpfn_worley2_different_seeds() {
    // Different seeds should produce different outputs
    float n1 = lpfn_worley(vec2(1.0, 1.0), 0u);
    float n2 = lpfn_worley(vec2(1.0, 1.0), 1u);
    return abs(n1 - n2) > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley2_different_seeds() == 1.0

float test_lpfn_worley3_different_seeds() {
    // Different seeds should produce different outputs
    float n1 = lpfn_worley(vec3(1.0, 1.0, 1.0), 0u);
    float n2 = lpfn_worley(vec3(1.0, 1.0, 1.0), 1u);
    return abs(n1 - n2) > 0.001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley3_different_seeds() == 1.0

// ============================================================================
// lpfn_worley_value() - Worley value variant
// ============================================================================

float test_lpfn_worley2_value_basic_range() {
    // Test 2D worley_value - should be in approximately [-1, 1] range
    float n = lpfn_worley_value(vec2(5.0, 3.0), 123u);
    return (n >= -1.5 && n <= 1.5) ? 1.0 : 0.0;
}

// run: test_lpfn_worley2_value_basic_range() == 1.0

float test_lpfn_worley3_value_basic_range() {
    // Test 3D worley_value - should be in approximately [-1, 1] range
    float n = lpfn_worley_value(vec3(1.0, 2.0, 3.0), 456u);
    return (n >= -1.5 && n <= 1.5) ? 1.0 : 0.0;
}

// run: test_lpfn_worley3_value_basic_range() == 1.0

float test_lpfn_worley2_value_determinism() {
    // Same inputs should produce same output
    float n1 = lpfn_worley_value(vec2(1.0, 1.0), 999u);
    float n2 = lpfn_worley_value(vec2(1.0, 1.0), 999u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley2_value_determinism() == 1.0

float test_lpfn_worley3_value_determinism() {
    // Same inputs should produce same output
    float n1 = lpfn_worley_value(vec3(1.0, 2.0, 3.0), 789u);
    float n2 = lpfn_worley_value(vec3(1.0, 2.0, 3.0), 789u);
    return abs(n1 - n2) < 0.0001 ? 1.0 : 0.0;
}

// run: test_lpfn_worley3_value_determinism() == 1.0

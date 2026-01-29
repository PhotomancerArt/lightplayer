// test run
// target riscv32.q32

// ============================================================================
// sqrt(): Square root function
// sqrt(x) returns √x
// Undefined if x < 0
// ============================================================================

float test_sqrt_zero() {
    // sqrt(0) should be 0
    return sqrt(0.0);
}

// run: test_sqrt_zero() ~= 0.0

float test_sqrt_one() {
    // sqrt(1) should be 1
    return sqrt(1.0);
}

// run: test_sqrt_one() ~= 1.0

float test_sqrt_four() {
    // sqrt(4) should be 2
    return sqrt(4.0);
}

// run: test_sqrt_four() ~= 2.0

float test_sqrt_nine() {
    // sqrt(9) should be 3
    return sqrt(9.0);
}

// run: test_sqrt_nine() ~= 3.0

float test_sqrt_two() {
    // sqrt(2) should be √2 ≈ 1.4142135623730951
    return sqrt(2.0);
}

// run: test_sqrt_two() ~= 1.4142135623730951

float test_sqrt_quarter() {
    // sqrt(0.25) should be 0.5
    return sqrt(0.25);
}

// run: test_sqrt_quarter() ~= 0.5

vec2 test_sqrt_vec2() {
    // Test with vec2
    return sqrt(vec2(0.0, 1.0));
}

// run: test_sqrt_vec2() ~= vec2(0.0, 1.0)

vec3 test_sqrt_vec3() {
    // Test with vec3
    return sqrt(vec3(1.0, 4.0, 9.0));
}

// run: test_sqrt_vec3() ~= vec3(1.0, 2.0, 3.0)

vec4 test_sqrt_vec4() {
    // Test with vec4
    return sqrt(vec4(0.0, 0.25, 1.0, 2.0));
}

// run: test_sqrt_vec4() ~= vec4(0.0, 0.5, 1.0, 1.4142135623730951)





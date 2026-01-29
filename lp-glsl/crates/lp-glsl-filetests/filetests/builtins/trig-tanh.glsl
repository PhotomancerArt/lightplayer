// test run
// target riscv32.q32

// ============================================================================
// tanh(): Hyperbolic tangent function
// tanh(x) = sinh(x) / cosh(x)
// ============================================================================

float test_tanh_zero() {
    // tanh(0) should be 0
    return tanh(0.0);
}

// run: test_tanh_zero() ~= 0.0

float test_tanh_one() {
    // tanh(1) should be approximately 0.7615941559557649
    return tanh(1.0);
}

// run: test_tanh_one() ~= 0.7615941559557649

float test_tanh_neg_one() {
    // tanh(-1) should be approximately -0.7615941559557649
    return tanh(-1.0);
}

// run: test_tanh_neg_one() ~= -0.7615941559557649

float test_tanh_two() {
    // tanh(2) should be approximately 0.9640275800758169
    return tanh(2.0);
}

// run: test_tanh_two() ~= 0.9640275800758169

float test_tanh_neg_two() {
    // tanh(-2) should be approximately -0.9640275800758169
    return tanh(-2.0);
}

// run: test_tanh_neg_two() ~= -0.9640275800758169

float test_tanh_half() {
    // tanh(0.5) should be approximately 0.46211715726000974
    return tanh(0.5);
}

// run: test_tanh_half() ~= 0.46211715726000974

vec2 test_tanh_vec2() {
    // Test with vec2
    return tanh(vec2(0.0, 1.0));
}

// run: test_tanh_vec2() ~= vec2(0.0, 0.7615941559557649)

vec3 test_tanh_vec3() {
    // Test with vec3
    return tanh(vec3(0.0, 1.0, -1.0));
}

// run: test_tanh_vec3() ~= vec3(0.0, 0.7615941559557649, -0.7615941559557649)

vec4 test_tanh_vec4() {
    // Test with vec4
    return tanh(vec4(0.0, 0.5, 1.0, -0.5));
}

// run: test_tanh_vec4() ~= vec4(0.0, 0.46211715726000974, 0.7615941559557649, -0.46211715726000974)





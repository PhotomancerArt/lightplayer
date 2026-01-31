// test run
// target riscv32.q32

// ============================================================================
// pow(): Power function
// pow(x, y) returns x^y
// Undefined if x < 0
// Undefined if x = 0 and y <= 0
// ============================================================================

float test_pow_two_two() {
    // pow(2, 2) should be 4
    return pow(2.0, 2.0);
}

// run: test_pow_two_two() ~= 4.0

float test_pow_three_two() {
    // pow(3, 2) should be 9
    return pow(3.0, 2.0);
}

// run: test_pow_three_two() ~= 9.0

float test_pow_two_half() {
    // pow(2, 0.5) should be √2 ≈ 1.4142135623730951
    return pow(2.0, 0.5);
}

// run: test_pow_two_half() ~= 1.4142135623730951

float test_pow_four_third() {
    // pow(4, 1/3) should be approximately 1.5874010519681994
    return pow(4.0, 0.3333333333333333);
}

// run: test_pow_four_third() ~= 1.5874010519681994

float test_pow_e_one() {
    // pow(e, 1) should be e ≈ 2.718281828459045
    return pow(2.718281828459045, 1.0);
}

// run: test_pow_e_one() ~= 2.718281828459045

float test_pow_two_neg_one() {
    // pow(2, -1) should be 0.5
    return pow(2.0, -1.0);
}

// run: test_pow_two_neg_one() ~= 0.5

vec2 test_pow_vec2() {
    // Test with vec2
    return pow(vec2(2.0, 3.0), vec2(2.0, 2.0));
}

// run: test_pow_vec2() ~= vec2(4.0, 9.0)

vec3 test_pow_vec3() {
    // Test with vec3
    return pow(vec3(2.0, 3.0, 4.0), vec3(0.5, 0.5, 0.5));
}

// run: test_pow_vec3() ~= vec3(1.4142135623730951, 1.7320508075688772, 2.0)

vec4 test_pow_vec4() {
    // Test with vec4
    return pow(vec4(2.0, 3.0, 4.0, 5.0), vec4(1.0, 1.0, 1.0, 1.0));
}

// run: test_pow_vec4() ~= vec4(2.0, 3.0, 4.0, 5.0)





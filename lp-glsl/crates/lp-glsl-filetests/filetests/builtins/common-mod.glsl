// test run
// target riscv32.q32

// ============================================================================
// mod(): Modulus function
// mod(x, y) returns x - y * floor(x / y)
// ============================================================================

float test_mod_positive_positive() {
    // mod(7.0, 3.0) should be 1.0 (7 - 3 * floor(7/3) = 7 - 3 * 2 = 1)
    return mod(7.0, 3.0);
}

// run: test_mod_positive_positive() ~= 1.0

float test_mod_positive_small() {
    // mod(5.0, 2.0) should be 1.0 (5 - 2 * floor(5/2) = 5 - 2 * 2 = 1)
    return mod(5.0, 2.0);
}

// run: test_mod_positive_small() ~= 1.0

float test_mod_exact() {
    // mod(6.0, 3.0) should be 0.0 (6 - 3 * floor(6/3) = 6 - 3 * 2 = 0)
    return mod(6.0, 3.0);
}

// run: test_mod_exact() ~= 0.0

float test_mod_negative_dividend() {
    // mod(-7.0, 3.0) should be 2.0 (-7 - 3 * floor(-7/3) = -7 - 3 * (-3) = 2)
    return mod(-7.0, 3.0);
}

// run: test_mod_negative_dividend() ~= 2.0

float test_mod_fractional() {
    // mod(7.5, 2.0) should be 1.5 (7.5 - 2 * floor(7.5/2) = 7.5 - 2 * 3 = 1.5)
    return mod(7.5, 2.0);
}

// run: test_mod_fractional() ~= 1.5

vec2 test_mod_vec2() {
    // Test with vec2
    return mod(vec2(7.0, 5.0), vec2(3.0, 2.0));
}

// run: test_mod_vec2() ~= vec2(1.0, 1.0)

vec3 test_mod_vec3() {
    // Test with vec3
    return mod(vec3(7.5, -7.0, 6.0), vec3(2.0, 3.0, 3.0));
}

// run: test_mod_vec3() ~= vec3(1.5, 2.0, 0.0)

vec4 test_mod_vec4() {
    // Test with vec4
    return mod(vec4(8.0, 5.0, -4.0, 9.0), vec4(3.0, 2.0, 5.0, 4.0));
}

// run: test_mod_vec4() ~= vec4(2.0, 1.0, 1.0, 1.0)





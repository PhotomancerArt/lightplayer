// test run
// target riscv32.q32

// ============================================================================
// ceil(): Ceiling function
// ceil(x) returns the nearest integer >= x
// ============================================================================

float test_ceil_integer() {
    // ceil(5.0) should be 5.0
    return ceil(5.0);
}

// run: test_ceil_integer() ~= 5.0

float test_ceil_positive() {
    // ceil(3.2) should be 4.0
    return ceil(3.2);
}

// run: test_ceil_positive() ~= 4.0

float test_ceil_negative() {
    // ceil(-2.3) should be -2.0
    return ceil(-2.3);
}

// run: test_ceil_negative() ~= -2.0

float test_ceil_negative_small() {
    // ceil(-0.1) should be 0.0
    return ceil(-0.1);
}

// run: test_ceil_negative_small() ~= 0.0

float test_ceil_half() {
    // ceil(2.5) should be 3.0
    return ceil(2.5);
}

// run: test_ceil_half() ~= 3.0

vec2 test_ceil_vec2() {
    // Test with vec2
    return ceil(vec2(1.1, -2.9));
}

// run: test_ceil_vec2() ~= vec2(2.0, -2.0)

vec3 test_ceil_vec3() {
    // Test with vec3
    return ceil(vec3(0.0, 3.2, -1.5));
}

// run: test_ceil_vec3() ~= vec3(0.0, 4.0, -1.0)

vec4 test_ceil_vec4() {
    // Test with vec4
    return ceil(vec4(1.1, 2.9, -0.5, 4.0));
}

// run: test_ceil_vec4() ~= vec4(2.0, 3.0, 0.0, 4.0)





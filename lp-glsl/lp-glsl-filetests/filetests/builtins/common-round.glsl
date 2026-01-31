// test run
// target riscv32.q32

// ============================================================================
// round(): Round to nearest integer function
// round(x) returns the nearest integer to x
// 0.5 rounds in implementation-defined direction
// ============================================================================

float test_round_integer() {
    // round(5.0) should be 5.0
    return round(5.0);
}

// run: test_round_integer() ~= 5.0

float test_round_up() {
    // round(3.7) should be 4.0
    return round(3.7);
}

// run: test_round_up() ~= 4.0

float test_round_down() {
    // round(3.2) should be 3.0
    return round(3.2);
}

// run: test_round_down() ~= 3.0

float test_round_half() {
    // round(2.5) implementation-defined (could be 2 or 3)
    return round(2.5);
}

// run: test_round_half() ~= 3.0

float test_round_neg_half() {
    // round(-2.5) implementation-defined (could be -2 or -3)
    return round(-2.5);
}

// run: test_round_neg_half() ~= -3.0

float test_round_negative() {
    // round(-1.7) should be -2.0
    return round(-1.7);
}

// run: test_round_negative() ~= -2.0

vec2 test_round_vec2() {
    // Test with vec2
    return round(vec2(1.4, -2.6));
}

// run: test_round_vec2() ~= vec2(1.0, -3.0)

vec3 test_round_vec3() {
    // Test with vec3
    return round(vec3(0.0, 3.7, -1.2));
}

// run: test_round_vec3() ~= vec3(0.0, 4.0, -1.0)

vec4 test_round_vec4() {
    // Test with vec4
    return round(vec4(1.1, 2.9, -0.5, 4.0));
}

// run: test_round_vec4() ~= vec4(1.0, 3.0, -1.0, 4.0)





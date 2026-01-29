// test run
// target riscv32.q32

// ============================================================================
// roundEven(): Round to nearest even integer function
// roundEven(x) returns the nearest even integer to x
// 0.5 rounds toward even (3.5 → 4.0, 4.5 → 4.0)
// ============================================================================

float test_roundeven_integer() {
    // roundEven(5.0) should be 5.0
    return roundEven(5.0);
}

// run: test_roundeven_integer() ~= 5.0

float test_roundeven_up() {
    // roundEven(3.7) should be 4.0
    return roundEven(3.7);
}

// run: test_roundeven_up() ~= 4.0

float test_roundeven_down() {
    // roundEven(3.2) should be 3.0
    return roundEven(3.2);
}

// run: test_roundeven_down() ~= 3.0

float test_roundeven_half_up() {
    // roundEven(3.5) should be 4.0 (rounds up to even)
    return roundEven(3.5);
}

// run: test_roundeven_half_up() ~= 4.0

float test_roundeven_half_down() {
    // roundEven(4.5) should be 4.0 (rounds down to even)
    return roundEven(4.5);
}

// run: test_roundeven_half_down() ~= 4.0

float test_roundeven_negative() {
    // roundEven(-1.7) should be -2.0
    return roundEven(-1.7);
}

// run: test_roundeven_negative() ~= -2.0

vec2 test_roundeven_vec2() {
    // Test with vec2
    return roundEven(vec2(3.5, 4.5));
}

// run: test_roundeven_vec2() ~= vec2(4.0, 4.0)

vec3 test_roundeven_vec3() {
    // Test with vec3
    return roundEven(vec3(0.0, 3.7, -1.2));
}

// run: test_roundeven_vec3() ~= vec3(0.0, 4.0, -1.0)

vec4 test_roundeven_vec4() {
    // Test with vec4
    return roundEven(vec4(1.1, 2.9, -0.5, 4.0));
}

// run: test_roundeven_vec4() ~= vec4(1.0, 3.0, -1.0, 4.0) [expect-fail]





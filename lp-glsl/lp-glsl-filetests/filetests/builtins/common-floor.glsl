// test run
// target riscv32.q32

// ============================================================================
// floor(): Floor function
// floor(x) returns the nearest integer <= x
// ============================================================================

float test_floor_integer() {
    // floor(5.0) should be 5.0
    return floor(5.0);
}

// run: test_floor_integer() ~= 5.0

float test_floor_positive() {
    // floor(3.7) should be 3.0
    return floor(3.7);
}

// run: test_floor_positive() ~= 3.0

float test_floor_negative() {
    // floor(-2.3) should be -3.0
    return floor(-2.3);
}

// run: test_floor_negative() ~= -3.0

float test_floor_negative_small() {
    // floor(-0.1) should be -1.0
    return floor(-0.1);
}

// run: test_floor_negative_small() ~= -1.0

float test_floor_half() {
    // floor(2.5) should be 2.0
    return floor(2.5);
}

// run: test_floor_half() ~= 2.0

vec2 test_floor_vec2() {
    // Test with vec2
    return floor(vec2(1.9, -2.1));
}

// run: test_floor_vec2() ~= vec2(1.0, -3.0)

vec3 test_floor_vec3() {
    // Test with vec3
    return floor(vec3(0.0, 3.7, -1.5));
}

// run: test_floor_vec3() ~= vec3(0.0, 3.0, -2.0)

vec4 test_floor_vec4() {
    // Test with vec4
    return floor(vec4(1.1, 2.9, -0.5, 4.0));
}

// run: test_floor_vec4() ~= vec4(1.0, 2.0, -1.0, 4.0)





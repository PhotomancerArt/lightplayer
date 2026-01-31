// test run
// target riscv32.q32

// ============================================================================
// radians(): Convert degrees to radians
// Formula: (π / 180) * degrees
// ============================================================================

float test_radians_zero() {
    // radians(0) should be 0
    return radians(0.0);
}

// run: test_radians_zero() ~= 0.0

float test_radians_ninety() {
    // radians(90) should be π/2
    return radians(90.0);
}

// run: test_radians_ninety() ~= 1.5707963267948966 (tolerance: 0.001)

float test_radians_one_eighty() {
    // radians(180) should be π
    return radians(180.0);
}

// run: test_radians_one_eighty() ~= 3.141592653589793 (tolerance: 0.001)

float test_radians_three_sixty() {
    // radians(360) should be 2π
    return radians(360.0);
}

// run: test_radians_three_sixty() ~= 6.283185307179586 (tolerance: 0.001)

float test_radians_negative() {
    // radians(-90) should be -π/2
    return radians(-90.0);
}

// run: test_radians_negative() ~= -1.5707963267948966 (tolerance: 0.001)

vec2 test_radians_vec2() {
    // Test with vec2
    return radians(vec2(0.0, 90.0));
}

// run: test_radians_vec2() ~= vec2(0.0, 1.5707963267948966) (tolerance: 0.001)

vec3 test_radians_vec3() {
    // Test with vec3
    return radians(vec3(0.0, 90.0, 180.0));
}

// run: test_radians_vec3() ~= vec3(0.0, 1.5707963267948966, 3.141592653589793) (tolerance: 0.001)

vec4 test_radians_vec4() {
    // Test with vec4
    return radians(vec4(0.0, 90.0, 180.0, 360.0));
}

// run: test_radians_vec4() ~= vec4(0.0, 1.5707963267948966, 3.141592653589793, 6.283185307179586) (tolerance: 0.001)





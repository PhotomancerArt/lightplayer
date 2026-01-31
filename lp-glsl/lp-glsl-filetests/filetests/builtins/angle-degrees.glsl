// test run
// target riscv32.q32

// ============================================================================
// degrees(): Convert radians to degrees
// Formula: (180 / π) * radians
// ============================================================================

float test_degrees_zero() {
    // degrees(0) should be 0
    return degrees(0.0);
}

// run: test_degrees_zero() ~= 0.0

float test_degrees_pi_half() {
    // degrees(π/2) should be 90
    return degrees(1.5707963267948966);
}

// run: test_degrees_pi_half() ~= 90.0 (tolerance: 0.001)

float test_degrees_pi() {
    // degrees(π) should be 180
    return degrees(3.141592653589793);
}

// run: test_degrees_pi() ~= 180.0 (tolerance: 0.001)

float test_degrees_two_pi() {
    // degrees(2π) should be 360
    return degrees(6.283185307179586);
}

// run: test_degrees_two_pi() ~= 360.0 (tolerance: 0.001)

float test_degrees_negative() {
    // degrees(-π/2) should be -90
    return degrees(-1.5707963267948966);
}

// run: test_degrees_negative() ~= -90.0 (tolerance: 0.001)

vec2 test_degrees_vec2() {
    // Test with vec2
    return degrees(vec2(0.0, 1.5707963267948966));
}

// run: test_degrees_vec2() ~= vec2(0.0, 90.0) (tolerance: 0.001)

vec3 test_degrees_vec3() {
    // Test with vec3
    return degrees(vec3(0.0, 1.5707963267948966, 3.141592653589793));
}

// run: test_degrees_vec3() ~= vec3(0.0, 90.0, 180.0) (tolerance: 0.001)

vec4 test_degrees_vec4() {
    // Test with vec4
    return degrees(vec4(0.0, 1.5707963267948966, 3.141592653589793, 6.283185307179586));
}

// run: test_degrees_vec4() ~= vec4(0.0, 90.0, 180.0, 360.0) (tolerance: 0.001)





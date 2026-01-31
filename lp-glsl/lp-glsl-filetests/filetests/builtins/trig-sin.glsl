// test run
// target riscv32.q32

// ============================================================================
// sin(): Sine function
// ============================================================================

float test_sin_zero() {
    // sin(0) should be 0
    return sin(0.0);
}

// run: test_sin_zero() ~= 0.0

float test_sin_pi_half() {
    // sin(π/2) should be 1
    return sin(1.5707963267948966);
}

// run: test_sin_pi_half() ~= 1.0

float test_sin_pi() {
    // sin(π) should be 0
    return sin(3.141592653589793);
}

// run: test_sin_pi() ~= 0.0 (tolerance: 0.01)

float test_sin_three_pi_half() {
    // sin(3π/2) should be -1
    return sin(4.71238898038469);
}

// run: test_sin_three_pi_half() ~= -1.0

float test_sin_two_pi() {
    // sin(2π) should be 0
    return sin(6.283185307179586);
}

// run: test_sin_two_pi() ~= 0.0

float test_sin_negative() {
    // sin(-π/2) should be -1
    return sin(-1.5707963267948966);
}

// run: test_sin_negative() ~= -1.0

float test_sin_fraction() {
    // sin(π/4) should be √2/2 ≈ 0.7071067811865476
    return sin(0.7853981633974483);
}

// run: test_sin_fraction() ~= 0.7071067811865476

vec2 test_sin_vec2() {
    // Test with vec2
    return sin(vec2(0.0, 1.5707963267948966));
}

// run: test_sin_vec2() ~= vec2(0.0, 1.0)

vec3 test_sin_vec3() {
    // Test with vec3
    return sin(vec3(0.0, 1.5707963267948966, 3.141592653589793));
}

// run: test_sin_vec3() ~= vec3(0.0, 1.0, 0.0) (tolerance: 0.01)

vec4 test_sin_vec4() {
    // Test with vec4
    return sin(vec4(0.0, 1.5707963267948966, 3.141592653589793, 4.71238898038469));
}

// run: test_sin_vec4() ~= vec4(0.0, 1.0, 0.0, -1.0) (tolerance: 0.01)




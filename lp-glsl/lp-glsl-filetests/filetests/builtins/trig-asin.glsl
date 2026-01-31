// test run
// target riscv32.q32

// ============================================================================
// asin(): Arc sine function
// Range: [-π/2, π/2]
// Undefined if |x| > 1
// ============================================================================

float test_asin_zero() {
    // asin(0) should be 0
    return asin(0.0);
}

// run: test_asin_zero() ~= 0.0

float test_asin_one() {
    // asin(1) should be π/2
    return asin(1.0);
}

// run: test_asin_one() ~= 1.5707963267948966

float test_asin_neg_one() {
    // asin(-1) should be -π/2
    return asin(-1.0);
}

// run: test_asin_neg_one() ~= -1.5707963267948966

float test_asin_half() {
    // asin(0.5) should be π/6 ≈ 0.5235987755982988
    return asin(0.5);
}

// run: test_asin_half() ~= 0.5235987755982988 (tolerance: 0.01)

float test_asin_neg_half() {
    // asin(-0.5) should be -π/6 ≈ -0.5235987755982988
    return asin(-0.5);
}

// run: test_asin_neg_half() ~= -0.5235987755982988 (tolerance: 0.01)

float test_asin_sqrt_half() {
    // asin(√2/2) should be π/4 ≈ 0.7853981633974483
    return asin(0.7071067811865476);
}

// run: test_asin_sqrt_half() ~= 0.7853981633974483

vec2 test_asin_vec2() {
    // Test with vec2
    return asin(vec2(0.0, 0.5));
}

// run: test_asin_vec2() ~= vec2(0.0, 0.5235987755982988) (tolerance: 0.01)

vec3 test_asin_vec3() {
    // Test with vec3
    return asin(vec3(0.0, 0.5, 1.0));
}

// run: test_asin_vec3() ~= vec3(0.0, 0.5235987755982988, 1.5707963267948966) (tolerance: 0.01)

vec4 test_asin_vec4() {
    // Test with vec4
    return asin(vec4(0.0, 0.5, 1.0, -0.5));
}

// run: test_asin_vec4() ~= vec4(0.0, 0.5235987755982988, 1.5707963267948966, -0.5235987755982988) (tolerance: 0.01)




// test run
// target riscv32.q32

// ============================================================================
// acos(): Arc cosine function
// Range: [0, π]
// Undefined if |x| > 1
// ============================================================================

float test_acos_one() {
    // acos(1) should be 0
    return acos(1.0);
}

// run: test_acos_one() ~= 0.0

float test_acos_zero() {
    // acos(0) should be π/2
    return acos(0.0);
}

// run: test_acos_zero() ~= 1.5707963267948966

float test_acos_neg_one() {
    // acos(-1) should be π
    return acos(-1.0);
}

// run: test_acos_neg_one() ~= 3.141592653589793

float test_acos_half() {
    // acos(0.5) should be π/3 ≈ 1.0471975511965976
    return acos(0.5);
}

// run: test_acos_half() ~= 1.0471975511965976 (tolerance: 0.01)

float test_acos_neg_half() {
    // acos(-0.5) should be 2π/3 ≈ 2.0943951023931953
    return acos(-0.5);
}

// run: test_acos_neg_half() ~= 2.0943951023931953 (tolerance: 0.01)

float test_acos_sqrt_half() {
    // acos(√2/2) should be π/4 ≈ 0.7853981633974483
    return acos(0.7071067811865476);
}

// run: test_acos_sqrt_half() ~= 0.7853981633974483

vec2 test_acos_vec2() {
    // Test with vec2
    return acos(vec2(1.0, 0.5));
}

// run: test_acos_vec2() ~= vec2(0.0, 1.0471975511965976) (tolerance: 0.01)

vec3 test_acos_vec3() {
    // Test with vec3
    return acos(vec3(1.0, 0.5, 0.0));
}

// run: test_acos_vec3() ~= vec3(0.0, 1.0471975511965976, 1.5707963267948966) (tolerance: 0.01)

vec4 test_acos_vec4() {
    // Test with vec4
    return acos(vec4(1.0, 0.5, 0.0, -0.5));
}

// run: test_acos_vec4() ~= vec4(0.0, 1.0471975511965976, 1.5707963267948966, 2.0943951023931953) (tolerance: 0.01)




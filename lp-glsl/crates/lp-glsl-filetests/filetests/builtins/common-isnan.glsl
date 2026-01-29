// test run
// target riscv32.q32

// ============================================================================
// isnan(): Is NaN function
// isnan(x) returns true if x is NaN
// ============================================================================

bool test_isnan_normal() {
    // isnan(1.0) should be false
    return isnan(1.0);
}

// run: test_isnan_normal() == false

bool test_isnan_zero() {
    // isnan(0.0) should be false
    return isnan(0.0);
}

// run: test_isnan_zero() == false

bool test_isnan_inf() {
    // isnan with infinity should be false
    return isnan(1.0 / 0.0);
}

// run: test_isnan_inf() == false

bool test_isnan_neg_inf() {
    // isnan with negative infinity should be false
    return isnan(-1.0 / 0.0);
}

// run: test_isnan_neg_inf() == false

bvec2 test_isnan_vec2() {
    // Test with vec2 - all should be false
    return isnan(vec2(1.0, -1.0));
}

// run: test_isnan_vec2() == bvec2(false, false)

bvec3 test_isnan_vec3() {
    // Test with vec3 - all should be false
    return isnan(vec3(0.0, 2.0, -2.0));
}

// run: test_isnan_vec3() == bvec3(false, false, false)

bvec4 test_isnan_vec4() {
    // Test with vec4 - all should be false
    return isnan(vec4(1.0, 0.0, -1.0, 3.0));
}

// run: test_isnan_vec4() == bvec4(false, false, false, false)





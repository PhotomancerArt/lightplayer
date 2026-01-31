// test run
// target riscv32.q32

// ============================================================================
// isinf(): Is infinity function
// isinf(x) returns true if x is positive or negative infinity
// ============================================================================

bool test_isinf_normal() {
    // isinf(1.0) should be false
    return isinf(1.0);
}

// run: test_isinf_normal() == false

bool test_isinf_zero() {
    // isinf(0.0) should be false
    return isinf(0.0);
}

// run: test_isinf_zero() == false

bool test_isinf_inf() {
    // isinf with positive infinity should be true
    return isinf(1.0 / 0.0);
}

// run: test_isinf_inf() == true

bool test_isinf_neg_inf() {
    // isinf with negative infinity should be true
    return isinf(-1.0 / 0.0);
}

// run: test_isinf_neg_inf() == true

bvec2 test_isinf_vec2() {
    // Test with vec2
    return isinf(vec2(1.0 / 0.0, 1.0));
}

// run: test_isinf_vec2() == bvec2(true, false)

bvec3 test_isinf_vec3() {
    // Test with vec3
    return isinf(vec3(1.0, -1.0 / 0.0, 2.0));
}

// run: test_isinf_vec3() == bvec3(false, true, false)

bvec4 test_isinf_vec4() {
    // Test with vec4
    return isinf(vec4(1.0 / 0.0, -1.0 / 0.0, 1.0, 0.0));
}

// run: test_isinf_vec4() == bvec4(true, true, false, false)





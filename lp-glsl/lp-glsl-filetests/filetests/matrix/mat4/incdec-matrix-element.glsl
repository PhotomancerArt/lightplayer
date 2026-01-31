// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m[0][0]) - mat4 elements
// ============================================================================

float test_preinc_mat4_element_00() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    float result = ++m[0][0];  // m[0][0] becomes 2.0, result is 2.0
    return result + m[0][0];  // Should be 2.0 + 2.0 = 4.0
}

// run: test_preinc_mat4_element_00() ~= 4.0

float test_preinc_mat4_element_33() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    float result = ++m[3][3];  // m[3][3] becomes 17.0, result is 17.0
    return result + m[3][3];  // Should be 17.0 + 17.0 = 34.0
}

// run: test_preinc_mat4_element_33() ~= 34.0

// ============================================================================
// Post-increment (m[0][0]++) - mat4 elements
// ============================================================================

float test_postinc_mat4_element_00() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    float old_val = m[0][0]++;  // m[0][0] becomes 2.0, old_val is 1.0
    return old_val + m[0][0];  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_mat4_element_00() ~= 3.0

// ============================================================================
// Pre-decrement (--m[0][0]) - mat4 elements
// ============================================================================

float test_predec_mat4_element_00() {
    mat4 m = mat4(
        3.0, 4.0, 5.0, 6.0,
        7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0,
        15.0, 16.0, 17.0, 18.0
    );
    float result = --m[0][0];  // m[0][0] becomes 2.0, result is 2.0
    return result + m[0][0];  // Should be 2.0 + 2.0 = 4.0
}

// run: test_predec_mat4_element_00() ~= 4.0

// ============================================================================
// Post-decrement (m[0][0]--) - mat4 elements
// ============================================================================

float test_postdec_mat4_element_00() {
    mat4 m = mat4(
        3.0, 4.0, 5.0, 6.0,
        7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0,
        15.0, 16.0, 17.0, 18.0
    );
    float old_val = m[0][0]--;  // m[0][0] becomes 2.0, old_val is 3.0
    return old_val + m[0][0];  // Should be 3.0 + 2.0 = 5.0
}

// run: test_postdec_mat4_element_00() ~= 5.0





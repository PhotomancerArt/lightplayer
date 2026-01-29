// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m[0][0]) - mat3 elements
// ============================================================================

float test_preinc_mat3_element_00() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    float result = ++m[0][0];  // m[0][0] becomes 2.0, result is 2.0
    return result + m[0][0];  // Should be 2.0 + 2.0 = 4.0
}

// run: test_preinc_mat3_element_00() ~= 4.0

float test_preinc_mat3_element_12() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    float result = ++m[1][2];  // m[1][2] becomes 7.0, result is 7.0
    return result + m[1][2];  // Should be 7.0 + 7.0 = 14.0
}

// run: test_preinc_mat3_element_12() ~= 14.0

float test_preinc_mat3_element_22() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    float result = ++m[2][2];  // m[2][2] becomes 10.0, result is 10.0
    return result + m[2][2];  // Should be 10.0 + 10.0 = 20.0
}

// run: test_preinc_mat3_element_22() ~= 20.0

// ============================================================================
// Post-increment (m[0][0]++) - mat3 elements
// ============================================================================

float test_postinc_mat3_element_00() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    float old_val = m[0][0]++;  // m[0][0] becomes 2.0, old_val is 1.0
    return old_val + m[0][0];  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_mat3_element_00() ~= 3.0

float test_postinc_mat3_element_12() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    float old_val = m[1][2]++;  // m[1][2] becomes 7.0, old_val is 6.0
    return old_val + m[1][2];  // Should be 6.0 + 7.0 = 13.0
}

// run: test_postinc_mat3_element_12() ~= 13.0

// ============================================================================
// Pre-decrement (--m[0][0]) - mat3 elements
// ============================================================================

float test_predec_mat3_element_00() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    float result = --m[0][0];  // m[0][0] becomes 2.0, result is 2.0
    return result + m[0][0];  // Should be 2.0 + 2.0 = 4.0
}

// run: test_predec_mat3_element_00() ~= 4.0

// ============================================================================
// Post-decrement (m[0][0]--) - mat3 elements
// ============================================================================

float test_postdec_mat3_element_00() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    float old_val = m[0][0]--;  // m[0][0] becomes 2.0, old_val is 3.0
    return old_val + m[0][0];  // Should be 3.0 + 2.0 = 5.0
}

// run: test_postdec_mat3_element_00() ~= 5.0





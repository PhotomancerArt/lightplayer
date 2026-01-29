// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m[0][0]) - mat2 elements
// ============================================================================

float test_preinc_mat2_element_00() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float result = ++m[0][0];  // m[0][0] becomes 2.0, result is 2.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_preinc_mat2_element_00() ~= 13.0

float test_preinc_mat2_element_01() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float result = ++m[0][1];  // m[0][1] becomes 3.0, result is 3.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_preinc_mat2_element_01() ~= 14.0

float test_preinc_mat2_element_10() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float result = ++m[1][0];  // m[1][0] becomes 4.0, result is 4.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_preinc_mat2_element_10() ~= 15.0

float test_preinc_mat2_element_11() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float result = ++m[1][1];  // m[1][1] becomes 5.0, result is 5.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_preinc_mat2_element_11() ~= 16.0

// ============================================================================
// Post-increment (m[0][0]++) - mat2 elements
// ============================================================================

float test_postinc_mat2_element_00() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float old_val = m[0][0]++;  // m[0][0] becomes 2.0, old_val is 1.0
    return old_val + m[0][0];  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_mat2_element_00() ~= 3.0

float test_postinc_mat2_element_01() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float old_val = m[0][1]++;  // m[0][1] becomes 3.0, old_val is 2.0
    return old_val + m[0][1];  // Should be 2.0 + 3.0 = 5.0
}

// run: test_postinc_mat2_element_01() ~= 5.0

float test_postinc_mat2_element_10() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float old_val = m[1][0]++;  // m[1][0] becomes 4.0, old_val is 3.0
    return old_val + m[1][0];  // Should be 3.0 + 4.0 = 7.0
}

// run: test_postinc_mat2_element_10() ~= 7.0

float test_postinc_mat2_element_11() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    float old_val = m[1][1]++;  // m[1][1] becomes 5.0, old_val is 4.0
    return old_val + m[1][1];  // Should be 4.0 + 5.0 = 9.0
}

// run: test_postinc_mat2_element_11() ~= 9.0

// ============================================================================
// Pre-decrement (--m[0][0]) - mat2 elements
// ============================================================================

float test_predec_mat2_element_00() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float result = --m[0][0];  // m[0][0] becomes 2.0, result is 2.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_predec_mat2_element_00() ~= 19.0

float test_predec_mat2_element_01() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float result = --m[0][1];  // m[0][1] becomes 3.0, result is 3.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_predec_mat2_element_01() ~= 20.0

float test_predec_mat2_element_10() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float result = --m[1][0];  // m[1][0] becomes 4.0, result is 4.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_predec_mat2_element_10() ~= 21.0

float test_predec_mat2_element_11() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float result = --m[1][1];  // m[1][1] becomes 5.0, result is 5.0
    return result + m[0][0] + m[0][1] + m[1][0] + m[1][1];
}

// run: test_predec_mat2_element_11() ~= 22.0

// ============================================================================
// Post-decrement (m[0][0]--) - mat2 elements
// ============================================================================

float test_postdec_mat2_element_00() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float old_val = m[0][0]--;  // m[0][0] becomes 2.0, old_val is 3.0
    return old_val + m[0][0];  // Should be 3.0 + 2.0 = 5.0
}

// run: test_postdec_mat2_element_00() ~= 5.0

float test_postdec_mat2_element_01() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float old_val = m[0][1]--;  // m[0][1] becomes 3.0, old_val is 4.0
    return old_val + m[0][1];  // Should be 4.0 + 3.0 = 7.0
}

// run: test_postdec_mat2_element_01() ~= 7.0

float test_postdec_mat2_element_10() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float old_val = m[1][0]--;  // m[1][0] becomes 4.0, old_val is 5.0
    return old_val + m[1][0];  // Should be 5.0 + 4.0 = 9.0
}

// run: test_postdec_mat2_element_10() ~= 9.0

float test_postdec_mat2_element_11() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    float old_val = m[1][1]--;  // m[1][1] becomes 5.0, old_val is 6.0
    return old_val + m[1][1];  // Should be 6.0 + 5.0 = 11.0
}

// run: test_postdec_mat2_element_11() ~= 11.0

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


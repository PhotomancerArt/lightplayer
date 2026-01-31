// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m[0]) - mat3 columns
// ============================================================================

float test_preinc_mat3_column_0() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 result = ++m[0];  // m[0] becomes (2.0, 3.0, 4.0), result is (2.0, 3.0, 4.0)
    return result.x + result.y + result.z;  // Should be 2.0 + 3.0 + 4.0 = 9.0
}

// run: test_preinc_mat3_column_0() ~= 9.0

float test_preinc_mat3_column_1() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 result = ++m[1];  // m[1] becomes (5.0, 6.0, 7.0), result is (5.0, 6.0, 7.0)
    return result.x + result.y + result.z;  // Should be 5.0 + 6.0 + 7.0 = 18.0
}

// run: test_preinc_mat3_column_1() ~= 18.0

float test_preinc_mat3_column_2() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 result = ++m[2];  // m[2] becomes (8.0, 9.0, 10.0), result is (8.0, 9.0, 10.0)
    return result.x + result.y + result.z;  // Should be 8.0 + 9.0 + 10.0 = 27.0
}

// run: test_preinc_mat3_column_2() ~= 27.0

// ============================================================================
// Post-increment (m[0]++) - mat3 columns
// ============================================================================

float test_postinc_mat3_column_0() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 old_col = m[0]++;  // m[0] becomes (2.0, 3.0, 4.0), old_col is (1.0, 2.0, 3.0)
    return old_col.x + old_col.y + old_col.z;  // Should be 1.0 + 2.0 + 3.0 = 6.0
}

// run: test_postinc_mat3_column_0() ~= 6.0

float test_postinc_mat3_column_1() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 old_col = m[1]++;  // m[1] becomes (5.0, 6.0, 7.0), old_col is (4.0, 5.0, 6.0)
    return old_col.x + old_col.y + old_col.z;  // Should be 4.0 + 5.0 + 6.0 = 15.0
}

// run: test_postinc_mat3_column_1() ~= 15.0

float test_postinc_mat3_column_2() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    vec3 old_col = m[2]++;  // m[2] becomes (8.0, 9.0, 10.0), old_col is (7.0, 8.0, 9.0)
    return old_col.x + old_col.y + old_col.z;  // Should be 7.0 + 8.0 + 9.0 = 24.0
}

// run: test_postinc_mat3_column_2() ~= 24.0

// ============================================================================
// Pre-decrement (--m[0]) - mat3 columns
// ============================================================================

float test_predec_mat3_column_0() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    vec3 result = --m[0];  // m[0] becomes (2.0, 3.0, 4.0), result is (2.0, 3.0, 4.0)
    return result.x + result.y + result.z;  // Should be 2.0 + 3.0 + 4.0 = 9.0
}

// run: test_predec_mat3_column_0() ~= 9.0

float test_predec_mat3_column_1() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    vec3 result = --m[1];  // m[1] becomes (5.0, 6.0, 7.0), result is (5.0, 6.0, 7.0)
    return result.x + result.y + result.z;  // Should be 5.0 + 6.0 + 7.0 = 18.0
}

// run: test_predec_mat3_column_1() ~= 18.0

float test_predec_mat3_column_2() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    vec3 result = --m[2];  // m[2] becomes (8.0, 9.0, 10.0), result is (8.0, 9.0, 10.0)
    return result.x + result.y + result.z;  // Should be 8.0 + 9.0 + 10.0 = 27.0
}

// run: test_predec_mat3_column_2() ~= 27.0

// ============================================================================
// Post-decrement (m[0]--) - mat3 columns
// ============================================================================

float test_postdec_mat3_column_0() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    vec3 old_col = m[0]--;  // m[0] becomes (2.0, 3.0, 4.0), old_col is (3.0, 4.0, 5.0)
    return old_col.x + old_col.y + old_col.z;  // Should be 3.0 + 4.0 + 5.0 = 12.0
}

// run: test_postdec_mat3_column_0() ~= 12.0

float test_postdec_mat3_column_1() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    vec3 old_col = m[1]--;  // m[1] becomes (5.0, 6.0, 7.0), old_col is (6.0, 7.0, 8.0)
    return old_col.x + old_col.y + old_col.z;  // Should be 6.0 + 7.0 + 8.0 = 21.0
}

// run: test_postdec_mat3_column_1() ~= 21.0

float test_postdec_mat3_column_2() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    vec3 old_col = m[2]--;  // m[2] becomes (8.0, 9.0, 10.0), old_col is (9.0, 10.0, 11.0)
    return old_col.x + old_col.y + old_col.z;  // Should be 9.0 + 10.0 + 11.0 = 30.0
}

// run: test_postdec_mat3_column_2() ~= 30.0





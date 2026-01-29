// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m[0]) - mat4 columns
// ============================================================================

float test_preinc_mat4_column_0() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    vec4 result = ++m[0];  // m[0] becomes (2.0, 3.0, 4.0, 5.0), result is (2.0, 3.0, 4.0, 5.0)
    return result.x + result.y + result.z + result.w;  // Should be 2.0 + 3.0 + 4.0 + 5.0 = 14.0
}

// run: test_preinc_mat4_column_0() ~= 14.0

float test_preinc_mat4_column_2() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    vec4 result = ++m[2];  // m[2] becomes (10.0, 11.0, 12.0, 13.0), result is (10.0, 11.0, 12.0, 13.0)
    return result.x + result.y + result.z + result.w;  // Should be 10.0 + 11.0 + 12.0 + 13.0 = 46.0
}

// run: test_preinc_mat4_column_2() ~= 46.0

// ============================================================================
// Post-increment (m[0]++) - mat4 columns
// ============================================================================

float test_postinc_mat4_column_0() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    vec4 old_col = m[0]++;  // m[0] becomes (2.0, 3.0, 4.0, 5.0), old_col is (1.0, 2.0, 3.0, 4.0)
    return old_col.x + old_col.y + old_col.z + old_col.w;  // Should be 1.0 + 2.0 + 3.0 + 4.0 = 10.0
}

// run: test_postinc_mat4_column_0() ~= 10.0

float test_postinc_mat4_column_3() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    vec4 old_col = m[3]++;  // m[3] becomes (14.0, 15.0, 16.0, 17.0), old_col is (13.0, 14.0, 15.0, 16.0)
    return old_col.x + old_col.y + old_col.z + old_col.w;  // Should be 13.0 + 14.0 + 15.0 + 16.0 = 58.0
}

// run: test_postinc_mat4_column_3() ~= 58.0

// ============================================================================
// Pre-decrement (--m[0]) - mat4 columns
// ============================================================================

float test_predec_mat4_column_0() {
    mat4 m = mat4(
        3.0, 4.0, 5.0, 6.0,
        7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0,
        15.0, 16.0, 17.0, 18.0
    );
    vec4 result = --m[0];  // m[0] becomes (2.0, 3.0, 4.0, 5.0), result is (2.0, 3.0, 4.0, 5.0)
    return result.x + result.y + result.z + result.w;  // Should be 2.0 + 3.0 + 4.0 + 5.0 = 14.0
}

// run: test_predec_mat4_column_0() ~= 14.0

// ============================================================================
// Post-decrement (m[0]--) - mat4 columns
// ============================================================================

float test_postdec_mat4_column_0() {
    mat4 m = mat4(
        3.0, 4.0, 5.0, 6.0,
        7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0,
        15.0, 16.0, 17.0, 18.0
    );
    vec4 old_col = m[0]--;  // m[0] becomes (2.0, 3.0, 4.0, 5.0), old_col is (3.0, 4.0, 5.0, 6.0)
    return old_col.x + old_col.y + old_col.z + old_col.w;  // Should be 3.0 + 4.0 + 5.0 + 6.0 = 18.0
}

// run: test_postdec_mat4_column_0() ~= 18.0





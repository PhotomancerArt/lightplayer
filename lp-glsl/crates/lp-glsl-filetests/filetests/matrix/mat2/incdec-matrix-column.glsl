// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m[0]) - mat2 columns
// ============================================================================

float test_preinc_mat2_column_0() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    vec2 result = ++m[0];  // m[0] becomes (2.0, 3.0), result is (2.0, 3.0)
    return result.x + result.y + m[0].x + m[0].y + m[1].x + m[1].y;
}

// run: test_preinc_mat2_column_0() ~= 17.0

float test_preinc_mat2_column_1() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    vec2 result = ++m[1];  // m[1] becomes (4.0, 5.0), result is (4.0, 5.0)
    return result.x + result.y + m[0].x + m[0].y + m[1].x + m[1].y;
}

// run: test_preinc_mat2_column_1() ~= 21.0

// ============================================================================
// Post-increment (m[0]++) - mat2 columns
// ============================================================================

float test_postinc_mat2_column_0() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    vec2 old_col = m[0]++;  // m[0] becomes (2.0, 3.0), old_col is (1.0, 2.0)
    return old_col.x + old_col.y;  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_mat2_column_0() ~= 3.0

float test_postinc_mat2_column_1() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    vec2 old_col = m[1]++;  // m[1] becomes (4.0, 5.0), old_col is (3.0, 4.0)
    return old_col.x + old_col.y;  // Should be 3.0 + 4.0 = 7.0
}

// run: test_postinc_mat2_column_1() ~= 7.0

// ============================================================================
// Pre-decrement (--m[0]) - mat2 columns
// ============================================================================

float test_predec_mat2_column_0() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    vec2 result = --m[0];  // m[0] becomes (2.0, 3.0), result is (2.0, 3.0)
    return result.x + result.y + m[0].x + m[0].y + m[1].x + m[1].y;
}

// run: test_predec_mat2_column_0() ~= 21.0

float test_predec_mat2_column_1() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    vec2 result = --m[1];  // m[1] becomes (4.0, 5.0), result is (4.0, 5.0)
    return result.x + result.y + m[0].x + m[0].y + m[1].x + m[1].y;
}

// run: test_predec_mat2_column_1() ~= 25.0

// ============================================================================
// Post-decrement (m[0]--) - mat2 columns
// ============================================================================

float test_postdec_mat2_column_0() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    vec2 old_col = m[0]--;  // m[0] becomes (2.0, 3.0), old_col is (3.0, 4.0)
    return old_col.x + old_col.y;  // Should be 3.0 + 4.0 = 7.0
}

// run: test_postdec_mat2_column_0() ~= 7.0

float test_postdec_mat2_column_1() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    vec2 old_col = m[1]--;  // m[1] becomes (4.0, 5.0), old_col is (5.0, 6.0)
    return old_col.x + old_col.y;  // Should be 5.0 + 6.0 = 11.0
}

// run: test_postdec_mat2_column_1() ~= 11.0





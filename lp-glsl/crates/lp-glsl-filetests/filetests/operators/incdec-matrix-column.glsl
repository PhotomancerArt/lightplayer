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







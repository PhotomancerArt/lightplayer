// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++m) - mat2
// ============================================================================

float test_preinc_mat2() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 result = ++m;  // m becomes incremented, result is the new value
    return result[0][0] + result[0][1] + result[1][0] + result[1][1];
}

// run: test_preinc_mat2() ~= 14.0

// ============================================================================
// Post-increment (m++) - mat2
// ============================================================================

float test_postinc_mat2() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 old_m = m++;  // m becomes mat2(2.0, 3.0, 4.0, 5.0), old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[1][0] + old_m[1][1];
}

// run: test_postinc_mat2() ~= 10.0

// ============================================================================
// Pre-decrement (--m) - mat2
// ============================================================================

float test_predec_mat2() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    mat2 result = --m;  // m becomes decremented, result is the new value
    return result[0][0] + result[0][1] + result[1][0] + result[1][1];
}

// run: test_predec_mat2() ~= 14.0

// ============================================================================
// Post-decrement (m--) - mat2
// ============================================================================

float test_postdec_mat2() {
    mat2 m = mat2(3.0, 4.0, 5.0, 6.0);
    mat2 old_m = m--;  // m becomes decremented, old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[1][0] + old_m[1][1];
}

// run: test_postdec_mat2() ~= 18.0

// ============================================================================
// Pre-increment (++m) - mat3
// ============================================================================

float test_preinc_mat3() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 result = ++m;  // m becomes incremented, result is the new value
    return result[0][0] + result[0][1] + result[0][2] +
           result[1][0] + result[1][1] + result[1][2] +
           result[2][0] + result[2][1] + result[2][2];
}

// run: test_preinc_mat3() ~= 54.0

// ============================================================================
// Post-increment (m++) - mat3
// ============================================================================

float test_postinc_mat3() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 old_m = m++;  // m becomes incremented, old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[0][2] +
           old_m[1][0] + old_m[1][1] + old_m[1][2] +
           old_m[2][0] + old_m[2][1] + old_m[2][2];
}

// run: test_postinc_mat3() ~= 45.0

// ============================================================================
// Pre-decrement (--m) - mat3
// ============================================================================

float test_predec_mat3() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    mat3 result = --m;  // m becomes decremented, result is the new value
    return result[0][0] + result[0][1] + result[0][2] +
           result[1][0] + result[1][1] + result[1][2] +
           result[2][0] + result[2][1] + result[2][2];
}

// run: test_predec_mat3() ~= 54.0

// ============================================================================
// Post-decrement (m--) - mat3
// ============================================================================

float test_postdec_mat3() {
    mat3 m = mat3(3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0);
    mat3 old_m = m--;  // m becomes decremented, old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[0][2] +
           old_m[1][0] + old_m[1][1] + old_m[1][2] +
           old_m[2][0] + old_m[2][1] + old_m[2][2];
}

// run: test_postdec_mat3() ~= 63.0

// ============================================================================
// Pre-increment (++m) - mat4
// ============================================================================

float test_preinc_mat4() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    mat4 result = ++m;  // m becomes incremented, result is the new value
    return result[0][0] + result[0][1] + result[0][2] + result[0][3] +
           result[1][0] + result[1][1] + result[1][2] + result[1][3] +
           result[2][0] + result[2][1] + result[2][2] + result[2][3] +
           result[3][0] + result[3][1] + result[3][2] + result[3][3];
}

// run: test_preinc_mat4() ~= 152.0

// ============================================================================
// Post-increment (m++) - mat4
// ============================================================================

float test_postinc_mat4() {
    mat4 m = mat4(
        1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0,
        13.0, 14.0, 15.0, 16.0
    );
    mat4 old_m = m++;  // m becomes incremented, old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[0][2] + old_m[0][3] +
           old_m[1][0] + old_m[1][1] + old_m[1][2] + old_m[1][3] +
           old_m[2][0] + old_m[2][1] + old_m[2][2] + old_m[2][3] +
           old_m[3][0] + old_m[3][1] + old_m[3][2] + old_m[3][3];
}

// run: test_postinc_mat4() ~= 136.0

// ============================================================================
// Pre-decrement (--m) - mat4
// ============================================================================

float test_predec_mat4() {
    mat4 m = mat4(
        3.0, 4.0, 5.0, 6.0,
        7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0,
        15.0, 16.0, 17.0, 18.0
    );
    mat4 result = --m;  // m becomes decremented, result is the new value
    return result[0][0] + result[0][1] + result[0][2] + result[0][3] +
           result[1][0] + result[1][1] + result[1][2] + result[1][3] +
           result[2][0] + result[2][1] + result[2][2] + result[2][3] +
           result[3][0] + result[3][1] + result[3][2] + result[3][3];
}

// run: test_predec_mat4() ~= 152.0

// ============================================================================
// Post-decrement (m--) - mat4
// ============================================================================

float test_postdec_mat4() {
    mat4 m = mat4(
        3.0, 4.0, 5.0, 6.0,
        7.0, 8.0, 9.0, 10.0,
        11.0, 12.0, 13.0, 14.0,
        15.0, 16.0, 17.0, 18.0
    );
    mat4 old_m = m--;  // m becomes decremented, old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[0][2] + old_m[0][3] +
           old_m[1][0] + old_m[1][1] + old_m[1][2] + old_m[1][3] +
           old_m[2][0] + old_m[2][1] + old_m[2][2] + old_m[2][3] +
           old_m[3][0] + old_m[3][1] + old_m[3][2] + old_m[3][3];
}

// run: test_postdec_mat4() ~= 168.0







// test run
// target riscv32.q32

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





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





// test error
// target riscv32.q32

void test_incdec_non_lvalue() {
    // This should fail - increment on a literal (not an lvalue)
    5++;
}

// EXPECT_ERROR_CODE: E0400
// EXPECT_ERROR: increment/decrement only supported on variables and vector components for now
// EXPECT_LOCATION: 4

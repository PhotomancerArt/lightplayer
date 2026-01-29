// test error
// target riscv32.q32

void test_incdec_nested() {
    int x = 5;
    // This should fail - result of post-increment is not an l-value
    (x++)++;
}

// EXPECT_ERROR_CODE: E0400
// EXPECT_ERROR: increment/decrement only supported on variables and vector components for now
// EXPECT_LOCATION: 6

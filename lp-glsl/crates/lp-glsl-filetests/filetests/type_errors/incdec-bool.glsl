// test error
// target riscv32.q32

void test_incdec_bool() {
    bool b = true;
    // This should fail - increment/decrement not allowed on bool
    b++;
}

// EXPECT_ERROR_CODE: E0112
// EXPECT_ERROR: post-increment requires numeric operand
// EXPECT_LOCATION: 4

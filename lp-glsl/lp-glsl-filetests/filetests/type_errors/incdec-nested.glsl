// test error
// target riscv32.q32

void test_incdec_nested() {
    int x = 5;
    // This should fail - result of post-increment is not an l-value
    (x++)++;  // expected-error E0115: {{expression is not a valid LValue}}
}

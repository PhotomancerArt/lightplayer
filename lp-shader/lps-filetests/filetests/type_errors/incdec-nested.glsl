// test error

void test_incdec_nested() {
    int x = 5;
    // This should fail - result of post-increment is not an l-value
    (x++)++;  // expected-error {{invalid increment target}}
}

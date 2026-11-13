// test error

void test_incdec_bool() {
    bool b = true;
    // This should fail - increment/decrement not allowed on bool
    b++;  // expected-error {{increment/decrement requires numeric local}}
}

// test error

void test_incdec_non_lvalue() {
    // This should fail - increment on a literal (not an lvalue)
    5++;  // expected-error E0115: {{expression is not a valid LValue}}
}

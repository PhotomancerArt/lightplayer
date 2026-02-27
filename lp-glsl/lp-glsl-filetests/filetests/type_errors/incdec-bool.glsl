// test error
// target riscv32.q32

void test_incdec_bool() {
    bool b = true;
    // This should fail - increment/decrement not allowed on bool
    b++;  // expected-error E0112: {{post-increment requires numeric operand}}
}

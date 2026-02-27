// test error
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// User-defined function cannot form constant expression.
// TODO: When const is implemented, expect error on init line; currently compiler
// ignores const decls and fails with undefined variable where BAD is used.

float get_val() { return 1.0; }
const float BAD = get_val();

float main() {
    return BAD;  // expected-error {{undefined variable}}
}

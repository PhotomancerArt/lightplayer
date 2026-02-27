// test error
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// User-defined function cannot form constant expression.

float get_val() { return 1.0; }
const float BAD = get_val();  // expected-error {{unknown constructor or non-const function}}

float main() {
    return BAD;  // expected-error {{undefined variable}}
}

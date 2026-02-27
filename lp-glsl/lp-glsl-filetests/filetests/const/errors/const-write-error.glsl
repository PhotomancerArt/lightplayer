// test error
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Writing to const is compile-time error.

float main() {
    const float x = 1.0;
    x = 2.0;  // expected-error {{cannot assign to const variable `x`}}
    return x;
}

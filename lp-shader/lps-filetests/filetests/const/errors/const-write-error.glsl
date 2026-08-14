// test error

// lps-glsl does not yet enforce the const qualifier on assignment (naga does).
// @unimplemented(frontend=lp)

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Writing to const is compile-time error.

float probe() {
    const float x = 1.0;
    x = 2.0;  // expected-error {{cannot assign to const variable `x`}}
    return x;
}

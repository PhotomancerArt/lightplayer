// test error

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// User-defined function cannot form constant expression.

float get_val() { return 1.0; }
const float BAD = get_val();  // expected-error {{unknown constructor or non-const function}}

// Naga stops after the const initializer error; `BAD` is never bound for a follow-up diagnostic.
float main() {
    return BAD;
}

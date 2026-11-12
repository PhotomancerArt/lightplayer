// test error

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Non-constant expression in const init must be rejected.

float non_const = 1.0;
const float BAD = non_const;  // expected-error {{not a constant expression: reads a non-const variable}}

// Compilation stops at the const initializer error; `BAD` is never bound for a follow-up diagnostic.
float probe() {
    return BAD;
}

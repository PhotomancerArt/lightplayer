// test error

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Non-constant expression in const init must be rejected.

float non_const = 1.0;
const float BAD = non_const;  // expected-error {{not a constant expression}}

// Naga stops after the const initializer error; `BAD` is never bound for a follow-up diagnostic.
float render() {
    return BAD;
}

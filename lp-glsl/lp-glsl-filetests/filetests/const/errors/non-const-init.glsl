// test error
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Non-constant expression in const init must be rejected.
// TODO: When const is implemented, expect error on init line; currently compiler
// ignores const decls and fails with undefined variable where BAD is used.

float non_const = 1.0;
const float BAD = non_const;

float main() {
    return BAD;  // expected-error {{undefined variable}}
}

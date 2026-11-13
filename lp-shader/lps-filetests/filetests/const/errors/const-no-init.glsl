// test error

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Const must be initialized at declaration.

const float BAD;  // expected-error {{const declaration requires initializer}}

float probe() {
    return 1.0;
}

// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Exponential builtins: pow, exp, log, sqrt, inversesqrt.

const float S = sqrt(4.0);

float test_builtin_sqrt() {
    return S;
}

// run: test_builtin_sqrt() ~= 2.0 [expect-fail]

const float P = pow(2.0, 3.0);

float test_builtin_pow() {
    return P;
}

// run: test_builtin_pow() ~= 8.0 [expect-fail]

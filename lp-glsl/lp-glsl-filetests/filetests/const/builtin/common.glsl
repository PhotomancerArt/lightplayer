// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Common builtins: abs, sign, floor, min, max, clamp.

const float A = abs(-1.5);

float test_builtin_abs() {
    return A;
}

// run: test_builtin_abs() ~= 1.5 [expect-fail]

const float M = min(1.0, 2.0);

float test_builtin_min() {
    return M;
}

// run: test_builtin_min() ~= 1.0 [expect-fail]

const float C = clamp(5.0, 0.0, 1.0);

float test_builtin_clamp() {
    return C;
}

// run: test_builtin_clamp() ~= 1.0 [expect-fail]

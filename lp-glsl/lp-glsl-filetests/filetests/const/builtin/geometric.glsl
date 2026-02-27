// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Geometric builtins: length, dot, normalize.

const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const float LENGTH_UNIT = length(UNIT_VECTOR);

float test_builtin_length() {
    return LENGTH_UNIT;
}

// run: test_builtin_length() ~= 1.0 [expect-fail]

const vec2 A = vec2(1.0, 0.0);
const vec2 B = vec2(0.0, 1.0);
const float D = dot(A, B);

float test_builtin_dot() {
    return D;
}

// run: test_builtin_dot() ~= 0.0 [expect-fail]

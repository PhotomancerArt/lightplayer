// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Trig builtins: radians, degrees, sin, cos, asin, acos.

const float DEG180 = 180.0;
const float RAD180 = radians(DEG180);

float test_builtin_radians() {
    return RAD180;
}

// run: test_builtin_radians() ~= 3.14159 [expect-fail]

const float HALF_PI = 1.570795;
const float S = sin(HALF_PI);

float test_builtin_sin() {
    return S;
}

// run: test_builtin_sin() ~= 1.0 [expect-fail]

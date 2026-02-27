// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Const references other const (ordering, dependency).

const float PI = 3.14159;
const float PI_OVER_TWO = PI / 2.0;
const float QUARTER_PI = PI_OVER_TWO / 2.0;
const float EIGHTH_PI = QUARTER_PI / 2.0;

float test_reference_nested() {
    return QUARTER_PI + EIGHTH_PI;
}

// run: test_reference_nested() ~= 1.9635 [expect-fail]

const vec3 UP = vec3(0.0, 1.0, 0.0);
const vec3 RIGHT = vec3(1.0, 0.0, 0.0);
const vec3 FORWARD = vec3(0.0, 0.0, 1.0);
const vec3 BASIS_SUM = UP + RIGHT + FORWARD;
const vec3 SCALED_BASIS = BASIS_SUM * 0.5;
const vec3 OFFSET_BASIS = SCALED_BASIS + vec3(0.1, 0.1, 0.1);

vec3 test_reference_complex() {
    return OFFSET_BASIS;
}

// run: test_reference_complex() ~= vec3(0.6, 0.6, 0.6) [expect-fail]

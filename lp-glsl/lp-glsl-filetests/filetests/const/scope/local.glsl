// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Local const inside function body.

float test_local_const() {
    const float x = 1.0;
    return x;
}

// run: test_local_const() ~= 1.0

float test_local_const_calculated() {
    const float PI = 3.14159;
    const float RADIUS = 5.0;
    const float CIRCUMFERENCE = 2.0 * PI * RADIUS;
    return CIRCUMFERENCE;
}

// run: test_local_const_calculated() ~= 31.4159

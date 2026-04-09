// test run

// Several functions in one module calling each other.

float native_add(float a, float b) {
    return a + b;
}

float native_mul(float a, float b) {
    return a * b;
}

float native_compute(float x) {
    return native_mul(native_add(x, 10.0), 2.0);
}

float test_native_multi_function() {
    return native_compute(5.0);
}

// run: test_native_multi_function() ~= 30.0

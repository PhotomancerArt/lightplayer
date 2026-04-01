// test run
// @unimplemented(backend=wasm)

float multiply(float, int count);

float multiply(float factor, int count) {
    float result = 1.0;
    for (int i = 0; i < count; i++) {
        result = result * factor;
    }
    return result;
}

float test_param_unnamed_mixed() {
    return multiply(2.0, 3);
}

// run: test_param_unnamed_mixed() ~= 8.0

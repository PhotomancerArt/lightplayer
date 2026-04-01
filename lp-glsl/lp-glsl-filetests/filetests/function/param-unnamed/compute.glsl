// test run
// @unimplemented(backend=wasm)

float compute(float a, float b, float c);

float compute(float a, float b, float c) {
    return a * b + c;
}

float test_param_unnamed_all_unnamed() {
    return compute(2.0, 3.0, 4.0);
}

// run: test_param_unnamed_all_unnamed() ~= 10.0

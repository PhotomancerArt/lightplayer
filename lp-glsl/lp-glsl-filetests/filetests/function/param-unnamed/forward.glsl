// test run

float complex_calc(float base, int exp, bool enable);

float complex_calc(float base, int exp, bool enable) {
    if (!enable) return 0.0;
    float result = 1.0;
    for (int i = 0; i < exp; i++) {
        result = result * base;
    }
    return result;
}

float test_param_unnamed_forward_declare() {
    float result1 = complex_calc(2.0, 3, true);
    float result2 = complex_calc(2.0, 3, true);
    return result1 + result2;
}

// run: test_param_unnamed_forward_declare() ~= 16.0

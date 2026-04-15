// test run

// Force a runtime select by using a non-constant condition
float test_select_runtime_true() {
    float a = 10.0;
    float b = 20.0;
    int idx = 0;
    // idx == 0 is true at runtime, so should select a
    return (idx == 0) ? a : b;  // Should be 10.0
}

// run: test_select_runtime_true() ~= 10.0

float test_select_runtime_false() {
    float a = 10.0;
    float b = 20.0;
    int idx = 1;
    // idx == 0 is false at runtime, so should select b
    return (idx == 0) ? a : b;  // Should be 20.0
}

// run: test_select_runtime_false() ~= 20.0

// test run

// User calls from if/else and from a for-loop.

float native_branch_helper(float x) {
    return x * 2.0;
}

float test_native_call_in_if() {
    float r;
    if (true) {
        r = native_branch_helper(5.0);
    } else {
        r = native_branch_helper(10.0);
    }
    return r;
}

// run: test_native_call_in_if() ~= 10.0

float native_loop_helper(float x) {
    return x + 1.0;
}

float test_native_call_in_loop() {
    float s = 0.0;
    for (int i = 0; i < 5; i++) {
        s = native_loop_helper(s);
    }
    return s;
}

// run: test_native_call_in_loop() ~= 5.0

// test run

// Six user float args (+ vmctx): register args a1–a7 on RV32 when no caller sret.

float sum_six(float a, float b, float c, float d, float e, float f) {
    return a + b + c + d + e + f;
}

float test_native_call_multi_args() {
    return sum_six(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
}

// run: test_native_call_multi_args() ~= 21.0

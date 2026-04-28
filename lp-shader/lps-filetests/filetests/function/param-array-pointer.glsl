// test run

// By-value `in float[4]` uses the pointer ABI: one pointer param + entry memcpy into a local slot.

float sum4(in float arr[4]) {
    return arr[0] + arr[1] + arr[2] + arr[3];
}

float test_param_array_pointer_sum() {
    float[4] x = float[4](1.0, 2.0, 3.0, 4.0);
    return sum4(x);
}

// run: test_param_array_pointer_sum() ~= 10.0

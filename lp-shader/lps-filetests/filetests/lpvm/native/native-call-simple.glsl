// compile-opt(inline.mode, never)

// test run

// Native / multi-backend: user function call, scalar float return (direct registers).

float helper_scalar(float x) {
    return x + 10.0;
}

float test_native_call_simple() {
    return helper_scalar(5.0);
}

// run: test_native_call_simple() ~= 15.0

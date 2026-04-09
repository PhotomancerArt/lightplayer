// test run

// Four-word return (sret on RV32): caller-side buffer + callee stores.

vec4 helper_vec4(float x) {
    return vec4(x, x * 2.0, x * 3.0, x * 4.0);
}

vec4 test_native_call_vec4_return() {
    return helper_vec4(5.0);
}

// run: test_native_call_vec4_return() ~= vec4(5.0, 10.0, 15.0, 20.0)

// compile-opt(inline.mode, never)

// test run

// Two scalar return words (a0–a1 direct return).

vec2 helper_vec2(float x) {
    return vec2(x, x * 2.0);
}

vec2 test_native_call_vec2_return() {
    return helper_vec2(5.0);
}

// run: test_native_call_vec2_return() ~= vec2(5.0, 10.0)

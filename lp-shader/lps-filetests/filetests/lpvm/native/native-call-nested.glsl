// compile-opt(inline.mode, never)

// test run

// Nested user calls (multiple callees in one expression).

float native_inner(float x) {
    return x * 2.0;
}

float native_outer(float x) {
    return native_inner(x) + 1.0;
}

float test_native_call_nested() {
    return native_outer(3.0);
}

// run: test_native_call_nested() ~= 7.0

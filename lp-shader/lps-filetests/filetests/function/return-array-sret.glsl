// test run

// sret return path: `return make_seq();` (supported). Local `T a = f();` from aggregate
// return is not lowered yet; see return-array tests.

float[4] make_seq() {
    return float[4](1.0, 2.0, 3.0, 4.0);
}

float[4] test_return_array_sret_seq() {
    return make_seq();
}

// run: test_return_array_sret_seq() ~= float[4](1.0, 2.0, 3.0, 4.0)

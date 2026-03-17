// test run
// @unimplemented(backend=wasm)

int test_for_loop_postinc() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_postinc() == 10

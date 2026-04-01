// test run

int test_postdec_scalar_int() {
    int x = 8;
    int old_x = x--;
    return old_x + x;  // Should return 8 + 7 = 15
}

// run: test_postdec_scalar_int() == 15

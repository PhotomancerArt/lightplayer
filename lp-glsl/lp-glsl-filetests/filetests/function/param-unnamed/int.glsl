// test run

int max_value(int a, int b);

int max_value(int a, int b) {
    return a > b ? a : b;
}

int test_param_unnamed_int() {
    return max_value(5, 8);
}

// run: test_param_unnamed_int() == 8

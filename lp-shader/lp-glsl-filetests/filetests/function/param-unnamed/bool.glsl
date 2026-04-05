// test run

bool both_true(bool a, bool b);

bool both_true(bool a, bool b) {
    return a && b;
}

bool test_param_unnamed_bool() {
    return both_true(true, false);
}

// run: test_param_unnamed_bool() == false

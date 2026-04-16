// test run

void process(float value, int count);

void process(float value, int count) {}

void test_param_unnamed_void() {
    process(5.0, 3);
}

// run: test_param_unnamed_void() == 0.0

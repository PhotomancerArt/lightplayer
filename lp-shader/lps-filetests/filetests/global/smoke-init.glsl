// test run

// Smoke test: global with constant initializer read back via __shader_init.

float g = 42.0;

float test_initialized_global() {
    return g;
}

// run: test_initialized_global() ~= 42.0

float test_initialized_mutate() {
    g = g + 1.0;
    return g;
}

// run: test_initialized_mutate() ~= 43.0

float test_initialized_mutate2() {
    g = g + 2.0;
    return g;
}

// run: test_initialized_mutate2() ~= 44.0

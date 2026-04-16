// test run

// Smoke test: simplest possible global variable read/write.

float g = 0.0;

float test_global_write_read() {
    g = 42.0;
    return g;
}

// run: test_global_write_read() ~= 42.0

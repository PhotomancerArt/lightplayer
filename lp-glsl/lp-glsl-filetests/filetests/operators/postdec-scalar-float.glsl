// test run
// target riscv32.q32

float test_postdec_scalar_float() {
    float x = 5.2;
    float old_x = x--;
    return old_x + x;  // Should be 5.2 + 4.2 = 9.4
}

// run: test_postdec_scalar_float() ~= 9.4

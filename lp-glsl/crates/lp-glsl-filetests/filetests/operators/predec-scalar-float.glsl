// test run
// target riscv32.q32

float test_predec_scalar_float() {
    float x = 5.5;
    float result = --x;  // Should decrement x to 4.5, then return 4.5
    return result;  // Should return 4.5
}

// run: test_predec_scalar_float() ~= 4.5

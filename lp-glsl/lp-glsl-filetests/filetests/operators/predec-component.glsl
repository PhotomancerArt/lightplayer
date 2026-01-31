// test run
// target riscv32.q32

float test_predec_component() {
    vec2 v = vec2(3.0, 4.0);
    float result = --v.y;  // v.y becomes 3.0, result is 3.0
    return result + v.x + v.y;  // Should be 3.0 + 3.0 + 3.0 = 9.0
}

// run: test_predec_component() ~= 9.0

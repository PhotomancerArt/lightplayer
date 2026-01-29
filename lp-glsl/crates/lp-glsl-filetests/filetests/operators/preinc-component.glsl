// test run
// target riscv32.q32

float test_preinc_component() {
    vec2 v = vec2(1.0, 2.0);
    float result = ++v.x;  // v.x becomes 2.0, result is 2.0
    return result + v.x + v.y;  // Should be 2.0 + 2.0 + 2.0 = 6.0
}

// run: test_preinc_component() ~= 6.0

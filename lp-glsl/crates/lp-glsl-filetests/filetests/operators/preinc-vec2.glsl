// test run
// target riscv32.q32

float test_preinc_vec2() {
    vec2 v = vec2(1.0, 2.0);
    vec2 result = ++v;  // Should increment v to (2.0, 3.0), then return (2.0, 3.0)
    return result.x + result.y;  // Should be 2.0 + 3.0 = 5.0
}

// run: test_preinc_vec2() ~= 5.0

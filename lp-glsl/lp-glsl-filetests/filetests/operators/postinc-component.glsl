// test run
// target riscv32.q32

float test_postinc_component() {
    vec2 v = vec2(1.0, 2.0);
    float old_x = v.x++;
    return old_x + v.x;  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_component() ~= 3.0

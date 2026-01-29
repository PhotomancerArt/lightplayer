// test run
// target riscv32.q32

float test_postinc_vec2() {
    vec2 v = vec2(1.0, 2.0);
    vec2 old_v = v++;
    return old_v.x + old_v.y;  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_vec2() ~= 3.0

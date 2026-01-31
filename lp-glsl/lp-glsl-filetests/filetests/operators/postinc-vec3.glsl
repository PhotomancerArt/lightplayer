// test run
// target riscv32.q32

int test_postinc_vec3() {
    ivec3 v = ivec3(5, 10, 15);
    ivec3 old_v = v++;
    return old_v.x + old_v.y + old_v.z;  // Should be 5 + 10 + 15 = 30
}

// run: test_postinc_vec3() == 30

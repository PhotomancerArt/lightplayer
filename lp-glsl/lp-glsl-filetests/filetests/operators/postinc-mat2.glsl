// test run
// target riscv32.q32

float test_postinc_mat2() {
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    mat2 old_m = m++;  // m becomes mat2(2.0, 3.0, 4.0, 5.0), old_m is original
    return old_m[0][0] + old_m[0][1] + old_m[1][0] + old_m[1][1];
}

// run: test_postinc_mat2() ~= 10.0

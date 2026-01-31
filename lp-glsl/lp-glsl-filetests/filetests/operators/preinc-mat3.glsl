// test run
// target riscv32.q32

float test_preinc_mat3() {
    mat3 m = mat3(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    mat3 result = ++m;  // m becomes incremented, result is the new value
    return result[0][0] + result[0][1] + result[0][2] +
           result[1][0] + result[1][1] + result[1][2] +
           result[2][0] + result[2][1] + result[2][2];
}

// run: test_preinc_mat3() ~= 54.0

// test run
//
// Forces heavy register pressure to trigger spilling.
// Each mat4 = 16 scalars. 5 mat4s = 80 values, exceeds available registers.

mat4 test_spill_many_mat4() {
    mat4 a = mat4(1.0);
    mat4 b = mat4(2.0);
    mat4 c = mat4(3.0);
    mat4 d = mat4(4.0);
    mat4 e = mat4(5.0);  // 80 scalars total
    return a + b + c + d + e;
}

// run: test_spill_many_mat4() ~= mat4(15.0)

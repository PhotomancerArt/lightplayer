// test run
//
// Forces heavy register pressure to trigger spilling.

mat2 test_spill_many_mat2() {
    mat2 a = mat2(1.0);
    mat2 b = mat2(2.0);
    mat2 c = mat2(3.0);
    mat2 d = mat2(4.0);
    return a + b + c + d;
}

// run: test_spill_many_mat2() ~= mat2(10.0)

// compile-opt(inline.mode, never)

// test run
//
// Performance: mat4 register pressure (16 scalars each).
// Each mat4 exceeds register file capacity. Tests measure spill
// efficiency for large aggregate types.

mat4 mat4_identity() {
    return mat4(1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0);
}

// Single mat4: baseline (16 values, all spills expected)
float test_single_mat4() {
    mat4 m = mat4_identity();
    return m[0][0] + m[1][1] + m[2][2] + m[3][3];
}

// Two mat4s with chained op: 32 scalars live
// Forces interleaved spill/reload pattern
float test_two_mat4_add() {
    mat4 a = mat4(1.0);
    mat4 b = mat4(2.0);
    mat4 c = a + b;  // element-wise add: 16 adds, heavy reg pressure
    return c[0][0];
}

// Three mat4s: 48 scalars in flight
float test_three_mat4_chain() {
    mat4 a = mat4(1.0);
    mat4 b = mat4(2.0);
    mat4 c = mat4(3.0);
    mat4 d = a + b + c;  // chained adds
    return d[0][0];
}

// run: test_single_mat4() == 4.0
// run: test_two_mat4_add() == 3.0
// run: test_three_mat4_chain() == 6.0

// compile-opt(inline.mode, never)

// test run

// Large sret (mat4): stress max callee buffer sizing on native path.

mat4 helper_identity_mat4() {
    return mat4(1.0);
}

float test_native_call_mat4_return() {
    mat4 m = helper_identity_mat4();
    return m[0][0] + m[1][1] + m[2][2] + m[3][3];
}

// run: test_native_call_mat4_return() ~= 4.0

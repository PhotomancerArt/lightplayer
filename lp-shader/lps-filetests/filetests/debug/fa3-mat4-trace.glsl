// test run

float mat4_trace(mat4 m) {
    return m[0][0] + m[1][1] + m[2][2] + m[3][3];
}

float test_mat4_trace() {
    return mat4_trace(mat4(
                      1.0, 0.0, 0.0, 0.0,
                      0.0, 2.0, 0.0, 0.0,
                      0.0, 0.0, 3.0, 0.0,
                      0.0, 0.0, 0.0, 4.0));
}

// run: test_mat4_trace() ~= 10.0

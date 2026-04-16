// test run
//
// Forces heavy register pressure to trigger spilling.

mat4 make_mat4() {
    return mat4(4.0);
}

mat4 test_return_mat4() {
    return make_mat4();
}
// run: test_return_mat4() ~= mat4(vec4(4.0, 0.0, 0.0, 0.0), vec4(0.0, 4.0, 0.0, 0.0), vec4(0.0, 0.0, 4.0, 0.0), vec4(0.0, 0.0, 0.0, 4.0))

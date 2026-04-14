// test run
//
// Forces heavy register pressure to trigger spilling.

vec2 spill_many_vec2(vec2 a, vec2 b) {
    vec2 c = vec2(3.0);
    vec2 d = vec2(4.0);
    vec2 e = vec2(5.0);
    return a + b + c + d + e;
}

vec2 test_spill_call_vec2() {
    return spill_many_vec2(
        vec2(1.0),
        vec2(2.0)
    ) + vec2(6.0);
}
// run: test_spill_call_vec2() ~= vec2(21.0, 21.0)


mat2 spill_many_mat2(mat2 a, mat2 b) {
    mat2 c = mat2(3.0);
    mat2 d = mat2(4.0);
    mat2 e = mat2(5.0);
    return a + b + c + d + e;
}

mat2 test_spill_call_mat2() {
    return spill_many_mat2(
        mat2(1.0),
        mat2(2.0)
    ) + mat2(6.0);
}
// run: test_spill_call_mat2() ~= mat2(vec2(21.0, 0.0), vec2(0.0, 21.0))


mat3 spill_many_mat3(mat3 a, mat3 b) {
    mat3 c = mat3(3.0);
    mat3 d = mat3(4.0);
    mat3 e = mat3(5.0);
    return a + b + c + d + e;
}

mat3 test_spill_call_mat3() {
    return spill_many_mat3(
        mat3(1.0),
        mat3(2.0)
    ) + mat3(6.0);
}
// run: test_spill_call_mat3() ~= mat3(vec3(21.0, 0.0, 0.0), vec3(0.0, 21.0, 0.0), vec3(0.0, 0.0, 21.0))


mat4 spill_many_mat4(mat4 a, mat4 b) {
    mat4 c = mat4(3.0);
    mat4 d = mat4(4.0);
    return a + b + c + d;
}

mat4 test_spill_call_mat4() {
    return spill_many_mat4(
        mat4(1.0),
        mat4(2.0)
    ) + mat4(5.0);
}
// run: test_spill_call_mat4() ~= mat4(vec4(15.0, 0.0, 0.0, 0.0), vec4(0.0, 15.0, 0.0, 0.0), vec4(0.0, 0.0, 15.0, 0.0), vec4(0.0, 0.0, 0.0, 15.0))

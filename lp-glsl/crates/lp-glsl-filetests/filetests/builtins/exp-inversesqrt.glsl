// test run
// target riscv32.q32

// ============================================================================
// inversesqrt(): Inverse square root function
// inversesqrt(x) returns 1/√x
// Undefined if x <= 0
// ============================================================================

float test_inversesqrt_one() {
    // inversesqrt(1) should be 1
    return inversesqrt(1.0);
}

// run: test_inversesqrt_one() ~= 1.0

float test_inversesqrt_four() {
    // inversesqrt(4) should be 0.5
    return inversesqrt(4.0);
}

// run: test_inversesqrt_four() ~= 0.5

float test_inversesqrt_nine() {
    // inversesqrt(9) should be 1/3 ≈ 0.3333333333333333
    return inversesqrt(9.0);
}

// run: test_inversesqrt_nine() ~= 0.3333333333333333

float test_inversesqrt_two() {
    // inversesqrt(2) should be 1/√2 ≈ 0.7071067811865476
    return inversesqrt(2.0);
}

// run: test_inversesqrt_two() ~= 0.7071067811865476

float test_inversesqrt_quarter() {
    // inversesqrt(0.25) should be 2
    return inversesqrt(0.25);
}

// run: test_inversesqrt_quarter() ~= 2.0

float test_inversesqrt_sixteenth() {
    // inversesqrt(0.0625) should be 4
    return inversesqrt(0.0625);
}

// run: test_inversesqrt_sixteenth() ~= 4.0

vec2 test_inversesqrt_vec2() {
    // Test with vec2
    return inversesqrt(vec2(1.0, 4.0));
}

// run: test_inversesqrt_vec2() ~= vec2(1.0, 0.5)

vec3 test_inversesqrt_vec3() {
    // Test with vec3
    return inversesqrt(vec3(1.0, 4.0, 9.0));
}

// run: test_inversesqrt_vec3() ~= vec3(1.0, 0.5, 0.3333333333333333)

vec4 test_inversesqrt_vec4() {
    // Test with vec4
    return inversesqrt(vec4(1.0, 0.25, 2.0, 0.0625));
}

// run: test_inversesqrt_vec4() ~= vec4(1.0, 2.0, 0.7071067811865476, 4.0)





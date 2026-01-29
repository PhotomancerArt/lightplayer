// test run
// target riscv32.q32

// ============================================================================
// fract(): Fractional part function
// fract(x) returns x - floor(x)
// ============================================================================

float test_fract_integer() {
    // fract(5.0) should be 0.0
    return fract(5.0);
}

// run: test_fract_integer() ~= 0.0

float test_fract_positive() {
    // fract(3.7) should be 0.7
    return fract(3.7);
}

// run: test_fract_positive() ~= 0.7

float test_fract_negative() {
    // fract(-2.3) should be 0.7 (since -2.3 - floor(-2.3) = -2.3 - (-3) = 0.7)
    return fract(-2.3);
}

// run: test_fract_negative() ~= 0.7

float test_fract_small() {
    // fract(0.1) should be 0.1
    return fract(0.1);
}

// run: test_fract_small() ~= 0.1

float test_fract_large() {
    // fract(7.9) should be 0.9
    return fract(7.9);
}

// run: test_fract_large() ~= 0.9

vec2 test_fract_vec2() {
    // Test with vec2
    return fract(vec2(1.4, -2.6));
}

// run: test_fract_vec2() ~= vec2(0.4, 0.4)

vec3 test_fract_vec3() {
    // Test with vec3
    return fract(vec3(0.0, 3.7, -1.2));
}

// run: test_fract_vec3() ~= vec3(0.0, 0.7, 0.8)

vec4 test_fract_vec4() {
    // Test with vec4
    return fract(vec4(1.1, 2.9, -0.5, 4.0));
}

// run: test_fract_vec4() ~= vec4(0.1, 0.9, 0.5, 0.0)





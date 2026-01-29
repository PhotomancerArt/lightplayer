// test run
// target riscv32.q32

// ============================================================================
// log(): Natural logarithm function
// log(x) returns ln(x)
// Undefined if x <= 0
// ============================================================================

float test_log_one() {
    // log(1) should be 0
    return log(1.0);
}

// run: test_log_one() ~= 0.0

float test_log_e() {
    // log(e) should be 1
    return log(2.718281828459045);
}

// run: test_log_e() ~= 1.0

float test_log_two() {
    // log(2) should be ln(2) ≈ 0.6931471805599453
    return log(2.0);
}

// run: test_log_two() ~= 0.6931471805599453

float test_log_ten() {
    // log(10) should be ln(10) ≈ 2.302585092994046
    return log(10.0);
}

// run: test_log_ten() ~= 2.302585092994046

float test_log_half() {
    // log(0.5) should be ln(0.5) ≈ -0.6931471805599453
    return log(0.5);
}

// run: test_log_half() ~= -0.6931471805599453

float test_log_sqrt_e() {
    // log(√e) should be 0.5
    return log(1.6487212711532444);
}

// run: test_log_sqrt_e() ~= 0.5

vec2 test_log_vec2() {
    // Test with vec2
    return log(vec2(1.0, 2.718281828459045));
}

// run: test_log_vec2() ~= vec2(0.0, 1.0)

vec3 test_log_vec3() {
    // Test with vec3
    return log(vec3(1.0, 2.0, 10.0));
}

// run: test_log_vec3() ~= vec3(0.0, 0.6931471805599453, 2.302585092994046)

vec4 test_log_vec4() {
    // Test with vec4
    return log(vec4(1.0, 2.0, 0.5, 0.1));
}

// run: test_log_vec4() ~= vec4(0.0, 0.6931471805599453, -0.6931471805599453, -2.302585092994046)





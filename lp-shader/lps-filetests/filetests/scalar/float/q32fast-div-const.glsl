// test run
// @ignore(float_mode=f32)
//
// Constant-divisor Q32 fast path: nonzero constants lower as
// lhs * q32(1.0 / rhs). This is the normal shader-speed path, not a
// helper-equivalent reciprocal divide.

float test_q32fast_div_const_half() {
    float a = 10.0;
    return a / 2.0;
}

// run: test_q32fast_div_const_half() ~= 5.0

float test_q32fast_div_const_third() {
    float a = 10.0;
    return a / 3.0;
}

// run: test_q32fast_div_const_third() ~= 3.3333 (tolerance: 0.002)

float test_q32fast_div_const_negative() {
    float a = 7.5;
    return a / -2.0;
}

// run: test_q32fast_div_const_negative() ~= -3.75

float test_q32fast_div_const_fractional() {
    float a = 1.5;
    return a / 0.25;
}

// run: test_q32fast_div_const_fractional() ~= 6.0

const float DIVISOR = 4.0;

float test_q32fast_div_const_name() {
    float a = 12.0;
    return a / DIVISOR;
}

// run: test_q32fast_div_const_name() ~= 3.0

float test_q32fast_div_const_vec_scalar() {
    vec3 a = vec3(8.0, 16.0, 32.0);
    vec3 r = a / 2.0;
    return r.x + r.y + r.z;
}

// run: test_q32fast_div_const_vec_scalar() ~= 28.0

float test_q32fast_div_const_vec_vector() {
    vec3 a = vec3(8.0, 16.0, 32.0);
    vec3 r = a / vec3(2.0, 4.0, 8.0);
    return r.x + r.y + r.z;
}

// run: test_q32fast_div_const_vec_vector() ~= 12.0

float test_q32fast_div_const_zero_saturates() {
    float a = 1.0;
    return a / 0.0;
}

// per-mode: the f32 channel asserts IEEE f32 results; Q32 keeps its saturation/wrapping expectation (M6 triage).
// run[q32]: test_q32fast_div_const_zero_saturates() ~= 32768.0 (tolerance: 0.02)

float test_q32fast_div_dynamic_stays_dynamic() {
    float a = 10.0;
    float b = 3.0;
    return a / b;
}

// run: test_q32fast_div_dynamic_stays_dynamic() ~= 3.3331 (tolerance: 0.002)

// test run

// ============================================================================
// mix(): Linear interpolation
// ============================================================================

float test_mix_float() {
    return mix(1.0, 8.0, 0.25);
}

// run: test_mix_float() ~= 2.75

float test_mix_float_t0() {
    return mix(1.0, 8.0, 0.0);
}

// run: test_mix_float_t0() ~= 1.0

float test_mix_float_t1() {
    return mix(1.0, 8.0, 1.0);
}

// run: test_mix_float_t1() ~= 8.0

vec3 test_mix_vec3() {
    return mix(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), 0.5);
}

// run: test_mix_vec3() ~= vec3(0.5, 0.5, 0.0)

vec3 test_mix_vec3_t0() {
    return mix(vec3(1.0, 2.0, 3.0), vec3(10.0, 20.0, 30.0), 0.0);
}

// run: test_mix_vec3_t0() ~= vec3(1.0, 2.0, 3.0)

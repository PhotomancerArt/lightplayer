// test run
// target riscv32.q32

// ============================================================================
// Component-wise operation verification tests
// Testing that operations are truly component-wise and independent
// ============================================================================

vec2 test_sin_component_wise() {
    // sin should be applied component-wise
    return sin(vec2(0.0, 1.5707963267948966));
}

// run: test_sin_component_wise() ~= vec2(0.0, 1.0)

vec3 test_cos_component_wise() {
    // cos should be applied component-wise
    return cos(vec3(0.0, 1.5707963267948966, 3.141592653589793));
}

// run: test_cos_component_wise() ~= vec3(1.0, 0.0, -1.0) (tolerance: 0.01)

vec4 test_exp_component_wise() {
    // exp should be applied component-wise
    return exp(vec4(0.0, 1.0, -1.0, 2.0));
}

// run: test_exp_component_wise() ~= vec4(1.0, 2.718281828459045, 0.36787944117144233, 7.38905609893065) (tolerance: 0.001)

vec2 test_sqrt_component_wise() {
    // sqrt should be applied component-wise
    return sqrt(vec2(1.0, 4.0));
}

// run: test_sqrt_component_wise() ~= vec2(1.0, 2.0)

vec3 test_log_component_wise() {
    // log should be applied component-wise
    return log(vec3(1.0, 2.718281828459045, 7.38905609893065));
}

// run: test_log_component_wise() ~= vec3(0.0, 1.0, 2.0)

vec4 test_sign_component_wise() {
    // sign should be applied component-wise
    return sign(vec4(2.0, 0.0, -1.5, -0.1));
}

// run: test_sign_component_wise() ~= vec4(1.0, 0.0, -1.0, -1.0)

vec2 test_floor_component_wise() {
    // floor should be applied component-wise
    return floor(vec2(1.9, -2.1));
}

// run: test_floor_component_wise() ~= vec2(1.0, -3.0)

vec3 test_ceil_component_wise() {
    // ceil should be applied component-wise
    return ceil(vec3(1.1, -2.9, 3.0));
}

// run: test_ceil_component_wise() ~= vec3(2.0, -2.0, 3.0)

vec4 test_fract_component_wise() {
    // fract should be applied component-wise
    return fract(vec4(1.7, -2.3, 0.0, 3.9));
}

// run: test_fract_component_wise() ~= vec4(0.7, 0.7, 0.0, 0.9)





// test run

layout(binding = 0) uniform float u_runtime_zero;

float rt(float x) { return x + u_runtime_zero; }

// ============================================================================
// step(): 0.0 below the edge, 1.0 at or above it
//
// step(edge, x) = x < edge ? 0.0 : 1.0, component-wise.
//
// Arguments are laundered through rt() so the operands are runtime values:
// constant-folded calls would exercise the folder, not the lowering.
// ============================================================================

float test_step_below() {
    return step(rt(0.5), rt(0.25));
}

// run: test_step_below() ~= 0.0

float test_step_above() {
    return step(rt(0.5), rt(0.75));
}

// run: test_step_above() ~= 1.0

float test_step_at_edge() {
    // x == edge is not below the edge, so the result is 1.0.
    return step(rt(0.5), rt(0.5));
}

// run: test_step_at_edge() ~= 1.0

float test_step_negative() {
    return step(rt(-1.0), rt(-2.0));
}

// run: test_step_negative() ~= 0.0

float test_step_zero_edge() {
    return step(rt(0.0), rt(0.0));
}

// run: test_step_zero_edge() ~= 1.0

vec2 test_step_vec2() {
    return step(vec2(rt(0.0), rt(1.0)), vec2(rt(-1.0), rt(2.0)));
}

// run: test_step_vec2() ~= vec2(0.0, 1.0)

vec3 test_step_vec3_scalar_edge() {
    // step(float edge, genType x) overload: the edge broadcasts.
    return step(rt(0.5), vec3(rt(0.0), rt(0.5), rt(1.0)));
}

// run: test_step_vec3_scalar_edge() ~= vec3(0.0, 1.0, 1.0)

vec4 test_step_vec4_component_wise() {
    return step(
        vec4(rt(1.0), rt(2.0), rt(3.0), rt(4.0)),
        vec4(rt(4.0), rt(1.0), rt(3.0), rt(0.0))
    );
}

// run: test_step_vec4_component_wise() ~= vec4(1.0, 0.0, 1.0, 0.0)

float test_step_constant_fold() {
    // Same builtin with compile-time-known operands.
    return step(0.5, 0.75);
}

// run: test_step_constant_fold() ~= 1.0

vec2 test_step_constant_vec2() {
    return step(vec2(0.0, 1.0), vec2(-1.0, 2.0));
}

// run: test_step_constant_vec2() ~= vec2(0.0, 1.0)

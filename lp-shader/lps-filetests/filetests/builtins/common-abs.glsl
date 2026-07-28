// test run

// ============================================================================
// abs(): absolute value
// Scalar float coverage; had no dedicated filetest until the 2026-07-27 wasm
// Q32 fabs stack-leak regression (see control/edge_cases/ops-in-nested-blocks.glsl
// for the in-block placements that actually triggered it).
// ============================================================================

float test_abs_positive() {
    return abs(3.5);
}

// run: test_abs_positive() ~= 3.5

float test_abs_negative() {
    return abs(-3.5);
}

// run: test_abs_negative() ~= 3.5

float test_abs_zero() {
    return abs(0.0);
}

// run: test_abs_zero() ~= 0.0

float test_abs_expr() {
    float a = 1.25;
    float b = 4.5;
    return abs(a - b);
}

// run: test_abs_expr() ~= 3.25

float test_abs_vec_component() {
    vec2 v = vec2(-2.5, 1.5);
    return abs(v.x) + abs(v.y);
}

// run: test_abs_vec_component() ~= 4.0

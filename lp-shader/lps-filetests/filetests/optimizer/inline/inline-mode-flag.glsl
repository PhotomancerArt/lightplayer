// compile-opt(inline.mode, always)

// test run

// ============================================================================
// Inliner: compile-opt(inline.mode, always) plumbs through; results match Auto.
// ============================================================================

float square(float x) {
    return x * x;
}

float add(float a, float b) {
    return a + b;
}

float compose(float x, float y) {
    return square(add(x, y));
}

float test_inline_mode_flag_chain() {
    return compose(2.0, 3.0);
}

// run: test_inline_mode_flag_chain() ~= 25.0

float test_inline_mode_flag_compose_small() {
    return compose(1.0, 1.0);
}

// run: test_inline_mode_flag_compose_small() ~= 4.0

float test_inline_mode_flag_square_of_sum() {
    return square(add(1.0, 2.0));
}

// run: test_inline_mode_flag_square_of_sum() ~= 9.0

// compile-opt(inline.mode, never)

// test run

// ============================================================================
// Exact copy of failing pattern from return-early.glsl
// ============================================================================

float absolute_value(float x) {
    if (x >= 0.0) {
        return x;
    }
    return -x;
}

float test_return_early_simple() {
    return absolute_value(-5.0);
}

// run: test_return_early_simple() ~= 5.0

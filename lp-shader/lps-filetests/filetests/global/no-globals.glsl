// test run

// ============================================================================
// No globals: Shader with zero globals and zero uniforms
// ============================================================================
// Verifies the fast path works (no init, no reset, no snapshot).

float test_no_globals_pure() {
    return 42.0;
}

// run: test_no_globals_pure() ~= 42.0

float test_no_globals_arithmetic(float a, float b) {
    return a * b + 1.0;
}

// run: test_no_globals_arithmetic(3.0, 4.0) ~= 13.0

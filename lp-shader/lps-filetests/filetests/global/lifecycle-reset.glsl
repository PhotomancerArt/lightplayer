// test run

// ============================================================================
// Lifecycle: Globals reset between calls
// ============================================================================
// Verifies that mutable globals are reset to their initialized values
// between consecutive calls (via the snapshot/restore lifecycle).

float counter = 0.0;

float test_reset_counter() {
    counter += 1.0;
    return counter;
}

// Both calls should return 1.0 — counter resets to 0.0 before each call.
// run: test_reset_counter() ~= 1.0
// @unimplemented(wasm.q32)
// run: test_reset_counter() ~= 1.0

float initialized_val = 5.0;

float test_reset_initialized() {
    float before = initialized_val;
    initialized_val = 99.0;
    return before;
}

// Should always read the initialized value (5.0), not the mutated one.
// @unimplemented(wasm.q32)
// run: test_reset_initialized() ~= 5.0
// @unimplemented(wasm.q32)
// run: test_reset_initialized() ~= 5.0

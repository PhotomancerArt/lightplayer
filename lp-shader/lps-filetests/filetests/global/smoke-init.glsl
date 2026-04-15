// test run

// Smoke test: global with constant initializer read back via __shader_init.

float g = 42.0;

float test_initialized_global() {
    return g;
}

// @unimplemented(wasm.q32)
// run: test_initialized_global() ~= 42.0

float test_initialized_mutate() {
    g = g + 1.0;
    return g;
}

// @unimplemented(wasm.q32)
// run: test_initialized_mutate() ~= 43.0

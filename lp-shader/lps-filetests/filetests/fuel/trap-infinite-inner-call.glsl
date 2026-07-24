// test run
// compile-opt(inline.mode, never)

// =============================================================================
// Fuel trap cascade: the infinite loop lives in a HELPER called from the test
// function (inlining pinned off so the call is real). rv32n/rv32lpn: the
// helper's back-edge check writes the trap code and aborts to its epilogue;
// back in the caller, fuel is still 0, so the caller's own checks (loop
// back-edge / next call's entry check) abort too — the trap cascades up the
// call stack and the host reads the trap slot after the outermost return.
// wasm: the helper's back-edge check writes the trap code and executes
// `unreachable`, which unwinds straight to the host in one shot (no cascade
// needed); the host reads the trap slot after the call returns.
// =============================================================================

int spin_helper() {
    int x = 0;
    while (true) {
        x = x + 1;
    }
    return x;
}

int call_spinning_helper() {
    int sum = 0;
    for (int i = 0; i < 4; i++) {
        sum = sum + spin_helper();
    }
    return sum;
}

// rv32c.q32: no fuel emission; dies on the emulator instruction limit (an error, not a trap)
// interp.f32: interp has no loop bound — this would hang the runner
// wgpu.f32: no fuel concept on the GPU tier
// @unsupported(rv32c.q32)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: call_spinning_helper() == 0
// EXPECT_TRAP: fuel exhausted

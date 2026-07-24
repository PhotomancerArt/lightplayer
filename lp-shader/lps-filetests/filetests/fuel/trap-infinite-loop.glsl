// test run

// =============================================================================
// Fuel trap: an unbounded loop exhausts the armed tank and aborts with a trap
// instead of hanging. rv32n/rv32lpn: the back-edge fuel check observes 0,
// writes trap code 1 to the vmctx trap slot, and jumps to the epilogue; the
// host reads the slot and reports "native trap: fuel exhausted (invocation N)".
// wasm: the emitted back-edge check writes the trap code and executes
// `unreachable`; the host reads the slot and reports
// "wasm trap: fuel exhausted (invocation N)".
// =============================================================================

int spin_forever() {
    int x = 0;
    while (true) {
        x = x + 1;
    }
    return x;
}

// rv32c.q32: no fuel emission; dies on the emulator instruction limit (an error, not a trap)
// interp.f32: interp has no loop bound — this would hang the runner
// wgpu.f32: no fuel concept on the GPU tier
// @unsupported(rv32c.q32)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: spin_forever() == 0
// EXPECT_TRAP: fuel exhausted

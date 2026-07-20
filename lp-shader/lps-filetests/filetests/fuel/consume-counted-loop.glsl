// test run

// =============================================================================
// Fuel unit semantics: one loop back-edge execution costs exactly 1 fuel unit;
// function entry checks are check-only (no decrement). Flat filetest calls arm
// lpvm::DEFAULT_VMCTX_FUEL (1_000_000) in the vmctx fuel low u32 before entry,
// so after N iterations `__lp_get_fuel()` reads exactly 1_000_000 - N.
// =============================================================================

int fuel_after_loop(int n) {
    int x = 0;
    for (int i = 0; i < n; i++) {
        x = x + 1;
    }
    if (x != n) {
        return -1;
    }
    return int(__lp_get_fuel());
}

// wasm.q32: wasmtime meters store fuel; the vmctx header word is never decremented
// rv32c.q32: cranelift reference backend does not emit fuel checks
// interp.f32: @vm::fuel is a VM-runtime import with no meaning in pure interpretation
// wgpu.f32: @vm::fuel does not exist on the GPU tier
// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: fuel_after_loop(0) == 1000000

// @unsupported(wasm.q32)
// @unsupported(rv32c.q32)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: fuel_after_loop(1000) == 999000

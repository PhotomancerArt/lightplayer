// test run

// =============================================================================
// VMContext: __lp_get_fuel reads header fuel (must match lp_glsl_abi::DEFAULT_VMCTX_FUEL)
// =============================================================================

// @unimplemented(rv32.q32): VMContext must be placed in emulator RAM, not host stack pointer
// @unimplemented(jit.q32): i32 VMContext word truncates on 64-bit hosts; needs 32-bit heap or mapping
// @unimplemented(wasm.q32): WASM harness does not yet set up VMContext pointer for imports

int test_read_vmctx_fuel() {
    return int(__lp_get_fuel());
}

// run: test_read_vmctx_fuel() == 1000000

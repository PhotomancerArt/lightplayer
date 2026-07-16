// test run

// =============================================================================
// VMContext: __lp_get_fuel reads header fuel (must match lpvm::DEFAULT_VMCTX_FUEL)
// =============================================================================


int test_read_vmctx_fuel() {
    return int(__lp_get_fuel());
}

// interp.f32: @vm::fuel is a VM-runtime import with no meaning in pure interpretation
// @unsupported(interp.f32)
// run: test_read_vmctx_fuel() == 1000000

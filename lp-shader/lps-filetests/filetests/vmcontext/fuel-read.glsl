// test run

// =============================================================================
// VMContext: __lp_get_fuel reads header fuel (must match lpvm::DEFAULT_VMCTX_FUEL)
// =============================================================================

// @unimplemented(jit.q32): i32 VMContext word truncates on 64-bit hosts; needs 32-bit heap or mapping

int test_read_vmctx_fuel() {
    return int(__lp_get_fuel());
}

// run: test_read_vmctx_fuel() == 1000000

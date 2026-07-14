// test run

// =============================================================================
// VMContext: __lp_get_fuel reads header fuel (must match lpvm::DEFAULT_VMCTX_FUEL)
// =============================================================================


int test_read_vmctx_fuel() {
    return int(__lp_get_fuel());
}

// run: test_read_vmctx_fuel() == 1000000

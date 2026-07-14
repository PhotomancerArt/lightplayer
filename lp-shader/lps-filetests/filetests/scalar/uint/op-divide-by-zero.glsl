// test run

// ============================================================================
// Unsigned division/remainder edge cases. LPIR follows RISC-V (RV32M)
// semantics on all targets — never traps (docs/design/lpir/02-core-ops.md):
//   x / 0u == 0xFFFFFFFFu (all ones)
//   x % 0u == x
// ============================================================================

uint div_uints(uint a, uint b) {
    return a / b;
}

// run: div_uints(42u, 0u) == 4294967295u
// run: div_uints(0u, 0u) == 4294967295u
// run: div_uints(4294967295u, 0u) == 4294967295u

uint mod_uints(uint a, uint b) {
    return a % b;
}

// run: mod_uints(42u, 0u) == 42u
// run: mod_uints(0u, 0u) == 0u
// run: mod_uints(4294967295u, 0u) == 4294967295u

uint test_uint_divide_by_zero_local() {
    uint a = 7u;
    uint b = 0u;
    return a / b;
}

// run: test_uint_divide_by_zero_local() == 4294967295u

uint test_uint_modulo_by_zero_local() {
    uint a = 7u;
    uint b = 0u;
    return a % b;
}

// run: test_uint_modulo_by_zero_local() == 7u

// Guard idiom: the eager RHS `x / i` must not trap when i == 0u.
uint guarded_div_uint(uint x, uint i) {
    if (i != 0u && (x / i > 0u)) {
        return 1u;
    }
    return 0u;
}

// run: guarded_div_uint(10u, 0u) == 0u
// run: guarded_div_uint(10u, 2u) == 1u
// run: guarded_div_uint(1u, 2u) == 0u

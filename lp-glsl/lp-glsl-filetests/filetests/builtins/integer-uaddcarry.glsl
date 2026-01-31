// test run
// target riscv32.q32

// ============================================================================
// uaddCarry(): Unsigned add with carry function
// uaddCarry(x, y, out carry) - unsigned add with carry
// Returns sum modulo 2^32, sets carry to 0 if sum < 2^32, 1 otherwise
// ============================================================================

uvec2 test_uaddcarry_uint_no_carry() {
    // uaddCarry(1, 2) should return (3, 0)
    uint carry;
    uint sum = uaddCarry(1u, 2u, carry);
    return uvec2(sum, carry);
}

// run: test_uaddcarry_uint_no_carry() == uvec2(3u, 0u) [expect-fail]

uvec2 test_uaddcarry_uint_with_carry() {
    // uaddCarry(max_uint, 1) should return (0, 1)
    uint carry;
    uint sum = uaddCarry(4294967295u, 1u, carry);
    return uvec2(sum, carry);
}

// run: test_uaddcarry_uint_with_carry() == uvec2(0u, 1u) [expect-fail]

uvec2 test_uaddcarry_uint_large_no_carry() {
    // uaddCarry(large numbers) without carry
    uint carry;
    uint sum = uaddCarry(2000000000u, 2000000000u, carry);
    return uvec2(sum, carry);
}

// run: test_uaddcarry_uint_large_no_carry() == uvec2(4000000000u, 0u) [expect-fail]

uvec4 test_uaddcarry_uvec2() {
    // uaddCarry with uvec2
    uvec2 carry;
    uvec2 sum = uaddCarry(uvec2(1u, 4294967295u), uvec2(2u, 1u), carry);
    return uvec4(sum.x, sum.y, carry.x, carry.y);
}

// run: test_uaddcarry_uvec2() == uvec4(3u, 0u, 0u, 1u) [expect-fail]





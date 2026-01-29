// test run
// target riscv32.q32

// ============================================================================
// Add: uint + uint -> uint
// ============================================================================

uint test_uint_add_positive_positive() {
    // Addition with positive unsigned integers
    return 5u + 3u;
}

// run: test_uint_add_positive_positive() == 8u

uint test_uint_add_zero() {
    return 42u + 0u;
}

// run: test_uint_add_zero() == 42u

uint test_uint_add_variables() {
    uint a = 15u;
    uint b = 27u;
    return a + b;
}

// run: test_uint_add_variables() == 42u

uint test_uint_add_expressions() {
    return (8u + 4u) + (6u + 2u);
}

// run: test_uint_add_expressions() == 20u

uint test_uint_add_in_assignment() {
    uint result = 5u;
    result = result + 10u;
    return result;
}

// run: test_uint_add_in_assignment() == 15u

uint test_uint_add_large_numbers() {
    return 100000u + 200000u;
}

// run: test_uint_add_large_numbers() == 300000u

uint test_uint_add_small_numbers() {
    return 1u + 2u;
}

// run: test_uint_add_small_numbers() == 3u

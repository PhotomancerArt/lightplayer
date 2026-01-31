// test run
// target riscv32.q32

// ============================================================================
// Divide: uint / uint -> uint (truncates toward zero)
// ============================================================================

uint test_uint_divide_positive_positive() {
    // Division with positive unsigned integers (truncates toward zero)
    return 10u / 3u;
}

// run: test_uint_divide_positive_positive() == 3u

uint test_uint_divide_by_one() {
    return 42u / 1u;
}

// run: test_uint_divide_by_one() == 42u

uint test_uint_divide_variables() {
    uint a = 20u;
    uint b = 4u;
    return a / b;
}

// run: test_uint_divide_variables() == 5u

uint test_uint_divide_expressions() {
    return (24u / 3u) / (8u / 2u);
}

// run: test_uint_divide_expressions() == 2u

uint test_uint_divide_in_assignment() {
    uint result = 15u;
    result = result / 3u;
    return result;
}

// run: test_uint_divide_in_assignment() == 5u

uint test_uint_divide_exact() {
    return 18u / 6u;
}

// run: test_uint_divide_exact() == 3u

uint test_uint_divide_remainder() {
    return 17u / 5u;
}

// run: test_uint_divide_remainder() == 3u

uint test_uint_divide_large_numbers() {
    return 1000000u / 1000u;
}

// run: test_uint_divide_large_numbers() == 1000u

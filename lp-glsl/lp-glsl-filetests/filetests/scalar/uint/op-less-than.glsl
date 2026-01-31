// test run
// target riscv32.q32

// ============================================================================
// Less Than: uint < uint -> bool
// ============================================================================

bool test_uint_less_than_simple() {
    // Simple less than comparison
    return 3u < 5u;
}

// run: test_uint_less_than_simple() == true

bool test_uint_less_than_equal() {
    return 5u < 5u;
}

// run: test_uint_less_than_equal() == false

bool test_uint_less_than_from_zero() {
    return 0u < 1u;
}

// run: test_uint_less_than_from_zero() == true

bool test_uint_less_than_variables() {
    uint a = 10u;
    uint b = 15u;
    return a < b;
}

// run: test_uint_less_than_variables() == true

bool test_uint_less_than_expressions() {
    return (2u + 3u) < (6u - 1u);
}

// run: test_uint_less_than_expressions() == false

bool test_uint_less_than_large_numbers() {
    return 100000u < 200000u;
}

// run: test_uint_less_than_large_numbers() == true

bool test_uint_less_than_small_numbers() {
    return 1u < 2u;
}

// run: test_uint_less_than_small_numbers() == true

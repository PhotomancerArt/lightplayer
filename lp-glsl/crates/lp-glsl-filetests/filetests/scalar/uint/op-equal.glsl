// test run
// target riscv32.q32

// ============================================================================
// Equal: uint == uint -> bool
// ============================================================================

bool test_uint_equal_same_values() {
    // Equality with same values
    return 5u == 5u;
}

// run: test_uint_equal_same_values() == true

bool test_uint_equal_different_values() {
    return 5u == 6u;
}

// run: test_uint_equal_different_values() == false

bool test_uint_equal_zero() {
    return 0u == 0u;
}

// run: test_uint_equal_zero() == true

bool test_uint_equal_variables_same() {
    uint a = 25u;
    uint b = 25u;
    return a == b;
}

// run: test_uint_equal_variables_same() == true

bool test_uint_equal_variables_different() {
    uint a = 25u;
    uint b = 26u;
    return a == b;
}

// run: test_uint_equal_variables_different() == false

bool test_uint_equal_expressions() {
    return (5u + 3u) == (2u * 4u);
}

// run: test_uint_equal_expressions() == true

bool test_uint_equal_self() {
    uint a = 42u;
    return a == a;
}

// run: test_uint_equal_self() == true

bool test_uint_equal_after_assignment() {
    uint a = 15u;
    uint b = 10u;
    b = a;
    return a == b;
}

// run: test_uint_equal_after_assignment() == true

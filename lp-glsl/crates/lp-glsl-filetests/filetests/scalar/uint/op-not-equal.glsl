// test run
// target riscv32.q32

// ============================================================================
// Not Equal: uint != uint -> bool
// ============================================================================

bool test_uint_not_equal_different_values() {
    // Inequality with different values
    return 5u != 6u;
}

// run: test_uint_not_equal_different_values() == true

bool test_uint_not_equal_same_values() {
    return 5u != 5u;
}

// run: test_uint_not_equal_same_values() == false

bool test_uint_not_equal_from_zero() {
    return 0u != 5u;
}

// run: test_uint_not_equal_from_zero() == true

bool test_uint_not_equal_variables_different() {
    uint a = 25u;
    uint b = 26u;
    return a != b;
}

// run: test_uint_not_equal_variables_different() == true

bool test_uint_not_equal_variables_same() {
    uint a = 25u;
    uint b = 25u;
    return a != b;
}

// run: test_uint_not_equal_variables_same() == false

bool test_uint_not_equal_expressions() {
    return (5u + 3u) != (2u * 5u);
}

// run: test_uint_not_equal_expressions() == true

bool test_uint_not_equal_self() {
    uint a = 42u;
    return a != a;
}

// run: test_uint_not_equal_self() == false

bool test_uint_not_equal_after_assignment() {
    uint a = 15u;
    uint b = 10u;
    b = a;
    return a != b;
}

// run: test_uint_not_equal_after_assignment() == false

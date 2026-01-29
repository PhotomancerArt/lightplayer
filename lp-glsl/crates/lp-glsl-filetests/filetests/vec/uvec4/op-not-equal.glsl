// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator -> bool (aggregate), notEqual(uvec4, uvec4) -> bvec4 (component-wise)
// ============================================================================

bool test_uvec4_not_equal_operator_true() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    // Operator != returns bool (aggregate comparison - any component differs)
    return a != b;
}

// run: test_uvec4_not_equal_operator_true() == true

bool test_uvec4_not_equal_operator_false() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(5u, 3u, 2u, 1u);
    return a != b;
}

// run: test_uvec4_not_equal_operator_false() == false

bool test_uvec4_not_equal_operator_partial_match() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(5u, 3u, 2u, 4u);
    return a != b;
}

// run: test_uvec4_not_equal_operator_partial_match() == true

bool test_uvec4_not_equal_operator_all_zero() {
    uvec4 a = uvec4(0u, 0u, 0u, 0u);
    uvec4 b = uvec4(1u, 0u, 0u, 0u);
    return a != b;
}

// run: test_uvec4_not_equal_operator_all_zero() == true

bool test_uvec4_not_equal_operator_max_values() {
    uvec4 a = uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u);
    uvec4 b = uvec4(4294967295u, 4294967295u, 4294967295u, 4294967294u);
    return a != b;
}

// run: test_uvec4_not_equal_operator_max_values() == true

bvec4 test_uvec4_not_equal_function() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(5u, 4u, 2u, 1u);
    // Function notEqual() returns bvec4 (component-wise comparison)
    return notEqual(a, b);
}

// run: test_uvec4_not_equal_function() == bvec4(false, true, false, false)

bvec4 test_uvec4_not_equal_function_all_false() {
    uvec4 a = uvec4(10u, 20u, 30u, 40u);
    uvec4 b = uvec4(10u, 20u, 30u, 40u);
    return notEqual(a, b);
}

// run: test_uvec4_not_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_uvec4_not_equal_function_all_true() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    return notEqual(a, b);
}

// run: test_uvec4_not_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_uvec4_not_equal_function_mixed() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 3u, 4u, 1u);
    return notEqual(a, b);
}

// run: test_uvec4_not_equal_function_mixed() == bvec4(true, false, true, false)

bool test_uvec4_not_equal_operator_self() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    return a != a;
}

// run: test_uvec4_not_equal_operator_self() == false

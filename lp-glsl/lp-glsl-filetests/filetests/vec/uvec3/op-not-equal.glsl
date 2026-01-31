// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator -> bool (aggregate), notEqual(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bool test_uvec3_not_equal_operator_true() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 4u, 1u);
    // Operator != returns bool (aggregate comparison - any component differs)
    return a != b;
}

// run: test_uvec3_not_equal_operator_true() == true

bool test_uvec3_not_equal_operator_false() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(5u, 3u, 2u);
    return a != b;
}

// run: test_uvec3_not_equal_operator_false() == false

bool test_uvec3_not_equal_operator_partial_match() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(5u, 3u, 4u);
    return a != b;
}

// run: test_uvec3_not_equal_operator_partial_match() == true

bool test_uvec3_not_equal_operator_all_zero() {
    uvec3 a = uvec3(0u, 0u, 0u);
    uvec3 b = uvec3(1u, 0u, 0u);
    return a != b;
}

// run: test_uvec3_not_equal_operator_all_zero() == true

bool test_uvec3_not_equal_operator_max_values() {
    uvec3 a = uvec3(4294967295u, 4294967295u, 4294967295u);
    uvec3 b = uvec3(4294967295u, 4294967295u, 4294967294u);
    return a != b;
}

// run: test_uvec3_not_equal_operator_max_values() == true

bvec3 test_uvec3_not_equal_function() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(5u, 4u, 2u);
    // Function notEqual() returns bvec3 (component-wise comparison)
    return notEqual(a, b);
}

// run: test_uvec3_not_equal_function() == bvec3(false, true, false)

bvec3 test_uvec3_not_equal_function_all_false() {
    uvec3 a = uvec3(10u, 20u, 30u);
    uvec3 b = uvec3(10u, 20u, 30u);
    return notEqual(a, b);
}

// run: test_uvec3_not_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_not_equal_function_all_true() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 4u, 1u);
    return notEqual(a, b);
}

// run: test_uvec3_not_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_not_equal_function_mixed() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 3u, 4u);
    return notEqual(a, b);
}

// run: test_uvec3_not_equal_function_mixed() == bvec3(true, false, true)

bool test_uvec3_not_equal_operator_self() {
    uvec3 a = uvec3(5u, 3u, 2u);
    return a != a;
}

// run: test_uvec3_not_equal_operator_self() == false

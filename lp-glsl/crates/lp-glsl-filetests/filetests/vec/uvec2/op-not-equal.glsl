// test run
// target riscv32.q32

// ============================================================================
// Not Equal: != operator -> bool (aggregate), notEqual(uvec2, uvec2) -> bvec2 (component-wise)
// ============================================================================

bool test_uvec2_not_equal_operator_true() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    // Operator != returns bool (aggregate comparison - any component differs)
    return a != b;
}

// run: test_uvec2_not_equal_operator_true() == true

bool test_uvec2_not_equal_operator_false() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 3u);
    return a != b;
}

// run: test_uvec2_not_equal_operator_false() == false

bool test_uvec2_not_equal_operator_partial_match() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 4u);
    return a != b;
}

// run: test_uvec2_not_equal_operator_partial_match() == true

bool test_uvec2_not_equal_operator_all_zero() {
    uvec2 a = uvec2(0u, 0u);
    uvec2 b = uvec2(1u, 0u);
    return a != b;
}

// run: test_uvec2_not_equal_operator_all_zero() == true

bool test_uvec2_not_equal_operator_max_values() {
    uvec2 a = uvec2(4294967295u, 4294967295u);
    uvec2 b = uvec2(4294967295u, 4294967294u);
    return a != b;
}

// run: test_uvec2_not_equal_operator_max_values() == true

bvec2 test_uvec2_not_equal_function() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 4u);
    // Function notEqual() returns bvec2 (component-wise comparison)
    return notEqual(a, b);
}

// run: test_uvec2_not_equal_function() == bvec2(false, true)

bvec2 test_uvec2_not_equal_function_all_false() {
    uvec2 a = uvec2(10u, 20u);
    uvec2 b = uvec2(10u, 20u);
    return notEqual(a, b);
}

// run: test_uvec2_not_equal_function_all_false() == bvec2(false, false)

bvec2 test_uvec2_not_equal_function_all_true() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    return notEqual(a, b);
}

// run: test_uvec2_not_equal_function_all_true() == bvec2(true, true)

bvec2 test_uvec2_not_equal_function_mixed() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 3u);
    return notEqual(a, b);
}

// run: test_uvec2_not_equal_function_mixed() == bvec2(true, false)

bool test_uvec2_not_equal_operator_self() {
    uvec2 a = uvec2(5u, 3u);
    return a != a;
}

// run: test_uvec2_not_equal_operator_self() == false

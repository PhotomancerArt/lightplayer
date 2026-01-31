// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec4/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(uvec4, uvec4) -> bvec4 (component-wise)
// ============================================================================

bool test_uvec4_equal_operator_true() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(5u, 3u, 2u, 1u);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_uvec4_equal_operator_true() == true

bool test_uvec4_equal_operator_false() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    return a == b;
}

// run: test_uvec4_equal_operator_false() == false

bool test_uvec4_equal_operator_partial_match() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(5u, 3u, 2u, 4u);
    return a == b;
}

// run: test_uvec4_equal_operator_partial_match() == false

bool test_uvec4_equal_operator_all_zero() {
    uvec4 a = uvec4(0u, 0u, 0u, 0u);
    uvec4 b = uvec4(0u, 0u, 0u, 0u);
    return a == b;
}

// run: test_uvec4_equal_operator_all_zero() == true

bool test_uvec4_equal_operator_after_assignment() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    b = a;
    return a == b;
}

// run: test_uvec4_equal_operator_after_assignment() == true

bvec4 test_uvec4_equal_function() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(5u, 4u, 2u, 1u);
    // Function equal() returns bvec4 (component-wise comparison)
    return equal(a, b);
}

// run: test_uvec4_equal_function() == bvec4(true, false, true, true)

bvec4 test_uvec4_equal_function_all_true() {
    uvec4 a = uvec4(10u, 20u, 30u, 40u);
    uvec4 b = uvec4(10u, 20u, 30u, 40u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_uvec4_equal_function_all_false() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 4u, 1u, 3u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_uvec4_equal_function_mixed() {
    uvec4 a = uvec4(5u, 3u, 2u, 1u);
    uvec4 b = uvec4(2u, 3u, 4u, 1u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_mixed() == bvec4(false, true, false, true)

bvec4 test_uvec4_equal_function_floats() {
    uvec4 a = uvec4(1u, 2u, 3u, 0u);
    uvec4 b = uvec4(1u, 2u, 3u, 0u);
    return equal(a, b);
}

// run: test_uvec4_equal_function_floats() == bvec4(true, true, true, true)

// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec3/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(uvec3, uvec3) -> bvec3 (component-wise)
// ============================================================================

bool test_uvec3_equal_operator_true() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(5u, 3u, 2u);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_uvec3_equal_operator_true() == true

bool test_uvec3_equal_operator_false() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 4u, 1u);
    return a == b;
}

// run: test_uvec3_equal_operator_false() == false

bool test_uvec3_equal_operator_partial_match() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(5u, 3u, 4u);
    return a == b;
}

// run: test_uvec3_equal_operator_partial_match() == false

bool test_uvec3_equal_operator_all_zero() {
    uvec3 a = uvec3(0u, 0u, 0u);
    uvec3 b = uvec3(0u, 0u, 0u);
    return a == b;
}

// run: test_uvec3_equal_operator_all_zero() == true

bool test_uvec3_equal_operator_after_assignment() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 4u, 1u);
    b = a;
    return a == b;
}

// run: test_uvec3_equal_operator_after_assignment() == true

bvec3 test_uvec3_equal_function() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(5u, 4u, 2u);
    // Function equal() returns bvec3 (component-wise comparison)
    return equal(a, b);
}

// run: test_uvec3_equal_function() == bvec3(true, false, true)

bvec3 test_uvec3_equal_function_all_true() {
    uvec3 a = uvec3(10u, 20u, 30u);
    uvec3 b = uvec3(10u, 20u, 30u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_uvec3_equal_function_all_false() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 4u, 1u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_uvec3_equal_function_mixed() {
    uvec3 a = uvec3(5u, 3u, 2u);
    uvec3 b = uvec3(2u, 3u, 4u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_mixed() == bvec3(false, true, false)

bvec3 test_uvec3_equal_function_floats() {
    uvec3 a = uvec3(1u, 2u, 3u);
    uvec3 b = uvec3(1u, 2u, 3u);
    return equal(a, b);
}

// run: test_uvec3_equal_function_floats() == bvec3(true, true, true)

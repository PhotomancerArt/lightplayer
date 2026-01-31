// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/uvec2/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(uvec2, uvec2) -> bvec2 (component-wise)
// ============================================================================

bool test_uvec2_equal_operator_true() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 3u);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_uvec2_equal_operator_true() == true

bool test_uvec2_equal_operator_false() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    return a == b;
}

// run: test_uvec2_equal_operator_false() == false

bool test_uvec2_equal_operator_partial_match() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 4u);
    return a == b;
}

// run: test_uvec2_equal_operator_partial_match() == false

bool test_uvec2_equal_operator_all_zero() {
    uvec2 a = uvec2(0u, 0u);
    uvec2 b = uvec2(0u, 0u);
    return a == b;
}

// run: test_uvec2_equal_operator_all_zero() == true

bool test_uvec2_equal_operator_after_assignment() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    b = a;
    return a == b;
}

// run: test_uvec2_equal_operator_after_assignment() == true

bvec2 test_uvec2_equal_function() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(5u, 4u);
    // Function equal() returns bvec2 (component-wise comparison)
    return equal(a, b);
}

// run: test_uvec2_equal_function() == bvec2(true, false)

bvec2 test_uvec2_equal_function_all_true() {
    uvec2 a = uvec2(10u, 20u);
    uvec2 b = uvec2(10u, 20u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_uvec2_equal_function_all_false() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 4u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_uvec2_equal_function_mixed() {
    uvec2 a = uvec2(5u, 3u);
    uvec2 b = uvec2(2u, 3u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_mixed() == bvec2(false, true)

bvec2 test_uvec2_equal_function_floats() {
    uvec2 a = uvec2(1u, 2u);
    uvec2 b = uvec2(1u, 2u);
    return equal(a, b);
}

// run: test_uvec2_equal_function_floats() == bvec2(true, true)

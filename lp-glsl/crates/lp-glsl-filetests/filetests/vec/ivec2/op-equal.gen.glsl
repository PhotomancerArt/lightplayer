// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec2/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(ivec2, ivec2) -> bvec2 (component-wise)
// ============================================================================

bool test_ivec2_equal_operator_true() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(5, 3);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_ivec2_equal_operator_true() == true

bool test_ivec2_equal_operator_false() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(2, 4);
    return a == b;
}

// run: test_ivec2_equal_operator_false() == false

bool test_ivec2_equal_operator_partial_match() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(5, 4);
    return a == b;
}

// run: test_ivec2_equal_operator_partial_match() == false

bool test_ivec2_equal_operator_all_zero() {
    ivec2 a = ivec2(0, 0);
    ivec2 b = ivec2(0, 0);
    return a == b;
}

// run: test_ivec2_equal_operator_all_zero() == true

bool test_ivec2_equal_operator_negative() {
    ivec2 a = ivec2(-5, -3);
    ivec2 b = ivec2(-5, -3);
    return a == b;
}

// run: test_ivec2_equal_operator_negative() == true

bool test_ivec2_equal_operator_after_assignment() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(2, 4);
    b = a;
    return a == b;
}

// run: test_ivec2_equal_operator_after_assignment() == true

bvec2 test_ivec2_equal_function() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(5, 4);
    // Function equal() returns bvec2 (component-wise comparison)
    return equal(a, b);
}

// run: test_ivec2_equal_function() == bvec2(true, false)

bvec2 test_ivec2_equal_function_all_true() {
    ivec2 a = ivec2(10, 20);
    ivec2 b = ivec2(10, 20);
    return equal(a, b);
}

// run: test_ivec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_ivec2_equal_function_all_false() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(2, 4);
    return equal(a, b);
}

// run: test_ivec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_ivec2_equal_function_mixed() {
    ivec2 a = ivec2(5, 3);
    ivec2 b = ivec2(2, 3);
    return equal(a, b);
}

// run: test_ivec2_equal_function_mixed() == bvec2(false, true)

bvec2 test_ivec2_equal_function_floats() {
    ivec2 a = ivec2(1, 2);
    ivec2 b = ivec2(1, 2);
    return equal(a, b);
}

// run: test_ivec2_equal_function_floats() == bvec2(true, true)

// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec4/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(ivec4, ivec4) -> bvec4 (component-wise)
// ============================================================================

bool test_ivec4_equal_operator_true() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(5, 3, 2, 1);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_ivec4_equal_operator_true() == true

bool test_ivec4_equal_operator_false() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(2, 4, 1, 3);
    return a == b;
}

// run: test_ivec4_equal_operator_false() == false

bool test_ivec4_equal_operator_partial_match() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(5, 3, 2, 4);
    return a == b;
}

// run: test_ivec4_equal_operator_partial_match() == false

bool test_ivec4_equal_operator_all_zero() {
    ivec4 a = ivec4(0, 0, 0, 0);
    ivec4 b = ivec4(0, 0, 0, 0);
    return a == b;
}

// run: test_ivec4_equal_operator_all_zero() == true

bool test_ivec4_equal_operator_negative() {
    ivec4 a = ivec4(-5, -3, -2, -1);
    ivec4 b = ivec4(-5, -3, -2, -1);
    return a == b;
}

// run: test_ivec4_equal_operator_negative() == true

bool test_ivec4_equal_operator_after_assignment() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(2, 4, 1, 3);
    b = a;
    return a == b;
}

// run: test_ivec4_equal_operator_after_assignment() == true

bvec4 test_ivec4_equal_function() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(5, 4, 2, 1);
    // Function equal() returns bvec4 (component-wise comparison)
    return equal(a, b);
}

// run: test_ivec4_equal_function() == bvec4(true, false, true, true)

bvec4 test_ivec4_equal_function_all_true() {
    ivec4 a = ivec4(10, 20, 30, 40);
    ivec4 b = ivec4(10, 20, 30, 40);
    return equal(a, b);
}

// run: test_ivec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_ivec4_equal_function_all_false() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(2, 4, 1, 3);
    return equal(a, b);
}

// run: test_ivec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_ivec4_equal_function_mixed() {
    ivec4 a = ivec4(5, 3, 2, 1);
    ivec4 b = ivec4(2, 3, 4, 1);
    return equal(a, b);
}

// run: test_ivec4_equal_function_mixed() == bvec4(false, true, false, true)

bvec4 test_ivec4_equal_function_floats() {
    ivec4 a = ivec4(1, 2, 3, 0);
    ivec4 b = ivec4(1, 2, 3, 0);
    return equal(a, b);
}

// run: test_ivec4_equal_function_floats() == bvec4(true, true, true, true)

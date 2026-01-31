// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/ivec3/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(ivec3, ivec3) -> bvec3 (component-wise)
// ============================================================================

bool test_ivec3_equal_operator_true() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(5, 3, 2);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_ivec3_equal_operator_true() == true

bool test_ivec3_equal_operator_false() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(2, 4, 1);
    return a == b;
}

// run: test_ivec3_equal_operator_false() == false

bool test_ivec3_equal_operator_partial_match() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(5, 3, 4);
    return a == b;
}

// run: test_ivec3_equal_operator_partial_match() == false

bool test_ivec3_equal_operator_all_zero() {
    ivec3 a = ivec3(0, 0, 0);
    ivec3 b = ivec3(0, 0, 0);
    return a == b;
}

// run: test_ivec3_equal_operator_all_zero() == true

bool test_ivec3_equal_operator_negative() {
    ivec3 a = ivec3(-5, -3, -2);
    ivec3 b = ivec3(-5, -3, -2);
    return a == b;
}

// run: test_ivec3_equal_operator_negative() == true

bool test_ivec3_equal_operator_after_assignment() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(2, 4, 1);
    b = a;
    return a == b;
}

// run: test_ivec3_equal_operator_after_assignment() == true

bvec3 test_ivec3_equal_function() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(5, 4, 2);
    // Function equal() returns bvec3 (component-wise comparison)
    return equal(a, b);
}

// run: test_ivec3_equal_function() == bvec3(true, false, true)

bvec3 test_ivec3_equal_function_all_true() {
    ivec3 a = ivec3(10, 20, 30);
    ivec3 b = ivec3(10, 20, 30);
    return equal(a, b);
}

// run: test_ivec3_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_ivec3_equal_function_all_false() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(2, 4, 1);
    return equal(a, b);
}

// run: test_ivec3_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_ivec3_equal_function_mixed() {
    ivec3 a = ivec3(5, 3, 2);
    ivec3 b = ivec3(2, 3, 4);
    return equal(a, b);
}

// run: test_ivec3_equal_function_mixed() == bvec3(false, true, false)

bvec3 test_ivec3_equal_function_floats() {
    ivec3 a = ivec3(1, 2, 3);
    ivec3 b = ivec3(1, 2, 3);
    return equal(a, b);
}

// run: test_ivec3_equal_function_floats() == bvec3(true, true, true)

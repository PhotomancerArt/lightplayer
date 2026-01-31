// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec4/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(vec4, vec4) -> bvec4 (component-wise)
// ============================================================================

bool test_vec4_equal_operator_true() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(5.0, 3.0, 2.0, 1.0);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_vec4_equal_operator_true() == true

bool test_vec4_equal_operator_false() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(2.0, 4.0, 1.0, 3.0);
    return a == b;
}

// run: test_vec4_equal_operator_false() == false

bool test_vec4_equal_operator_partial_match() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(5.0, 3.0, 2.0, 4.0);
    return a == b;
}

// run: test_vec4_equal_operator_partial_match() == false

bool test_vec4_equal_operator_all_zero() {
    vec4 a = vec4(0.0, 0.0, 0.0, 0.0);
    vec4 b = vec4(0.0, 0.0, 0.0, 0.0);
    return a == b;
}

// run: test_vec4_equal_operator_all_zero() == true

bool test_vec4_equal_operator_negative() {
    vec4 a = vec4(-5.0, -3.0, -2.0, -1.0);
    vec4 b = vec4(-5.0, -3.0, -2.0, -1.0);
    return a == b;
}

// run: test_vec4_equal_operator_negative() == true

bool test_vec4_equal_operator_after_assignment() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(2.0, 4.0, 1.0, 3.0);
    b = a;
    return a == b;
}

// run: test_vec4_equal_operator_after_assignment() == true

bvec4 test_vec4_equal_function() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(5.0, 4.0, 2.0, 1.0);
    // Function equal() returns bvec4 (component-wise comparison)
    return equal(a, b);
}

// run: test_vec4_equal_function() == bvec4(true, false, true, true)

bvec4 test_vec4_equal_function_all_true() {
    vec4 a = vec4(10.0, 20.0, 30.0, 40.0);
    vec4 b = vec4(10.0, 20.0, 30.0, 40.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_all_true() == bvec4(true, true, true, true)

bvec4 test_vec4_equal_function_all_false() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(2.0, 4.0, 1.0, 3.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_all_false() == bvec4(false, false, false, false)

bvec4 test_vec4_equal_function_mixed() {
    vec4 a = vec4(5.0, 3.0, 2.0, 1.0);
    vec4 b = vec4(2.0, 3.0, 4.0, 1.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_mixed() == bvec4(false, true, false, true)

bvec4 test_vec4_equal_function_floats() {
    vec4 a = vec4(1.0, 2.0, 3.0, 0.0);
    vec4 b = vec4(1.0, 2.0, 3.0, 0.0);
    return equal(a, b);
}

// run: test_vec4_equal_function_floats() == bvec4(true, true, true, true)

// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec3/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(vec3, vec3) -> bvec3 (component-wise)
// ============================================================================

bool test_vec3_equal_operator_true() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(5.0, 3.0, 2.0);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_vec3_equal_operator_true() == true

bool test_vec3_equal_operator_false() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(2.0, 4.0, 1.0);
    return a == b;
}

// run: test_vec3_equal_operator_false() == false

bool test_vec3_equal_operator_partial_match() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(5.0, 3.0, 4.0);
    return a == b;
}

// run: test_vec3_equal_operator_partial_match() == false

bool test_vec3_equal_operator_all_zero() {
    vec3 a = vec3(0.0, 0.0, 0.0);
    vec3 b = vec3(0.0, 0.0, 0.0);
    return a == b;
}

// run: test_vec3_equal_operator_all_zero() == true

bool test_vec3_equal_operator_negative() {
    vec3 a = vec3(-5.0, -3.0, -2.0);
    vec3 b = vec3(-5.0, -3.0, -2.0);
    return a == b;
}

// run: test_vec3_equal_operator_negative() == true

bool test_vec3_equal_operator_after_assignment() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(2.0, 4.0, 1.0);
    b = a;
    return a == b;
}

// run: test_vec3_equal_operator_after_assignment() == true

bvec3 test_vec3_equal_function() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(5.0, 4.0, 2.0);
    // Function equal() returns bvec3 (component-wise comparison)
    return equal(a, b);
}

// run: test_vec3_equal_function() == bvec3(true, false, true)

bvec3 test_vec3_equal_function_all_true() {
    vec3 a = vec3(10.0, 20.0, 30.0);
    vec3 b = vec3(10.0, 20.0, 30.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_all_true() == bvec3(true, true, true)

bvec3 test_vec3_equal_function_all_false() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(2.0, 4.0, 1.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_all_false() == bvec3(false, false, false)

bvec3 test_vec3_equal_function_mixed() {
    vec3 a = vec3(5.0, 3.0, 2.0);
    vec3 b = vec3(2.0, 3.0, 4.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_mixed() == bvec3(false, true, false)

bvec3 test_vec3_equal_function_floats() {
    vec3 a = vec3(1.0, 2.0, 3.0);
    vec3 b = vec3(1.0, 2.0, 3.0);
    return equal(a, b);
}

// run: test_vec3_equal_function_floats() == bvec3(true, true, true)

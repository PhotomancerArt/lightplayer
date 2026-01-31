// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app vec/vec2/op-equal --write
//
// test run
// target riscv32.q32

// ============================================================================
// Equal: == operator -> bool (aggregate), equal(vec2, vec2) -> bvec2 (component-wise)
// ============================================================================

bool test_vec2_equal_operator_true() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(5.0, 3.0);
    // Operator == returns bool (aggregate comparison - all components must match)
    return a == b;
}

// run: test_vec2_equal_operator_true() == true

bool test_vec2_equal_operator_false() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(2.0, 4.0);
    return a == b;
}

// run: test_vec2_equal_operator_false() == false

bool test_vec2_equal_operator_partial_match() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(5.0, 4.0);
    return a == b;
}

// run: test_vec2_equal_operator_partial_match() == false

bool test_vec2_equal_operator_all_zero() {
    vec2 a = vec2(0.0, 0.0);
    vec2 b = vec2(0.0, 0.0);
    return a == b;
}

// run: test_vec2_equal_operator_all_zero() == true

bool test_vec2_equal_operator_negative() {
    vec2 a = vec2(-5.0, -3.0);
    vec2 b = vec2(-5.0, -3.0);
    return a == b;
}

// run: test_vec2_equal_operator_negative() == true

bool test_vec2_equal_operator_after_assignment() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(2.0, 4.0);
    b = a;
    return a == b;
}

// run: test_vec2_equal_operator_after_assignment() == true

bvec2 test_vec2_equal_function() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(5.0, 4.0);
    // Function equal() returns bvec2 (component-wise comparison)
    return equal(a, b);
}

// run: test_vec2_equal_function() == bvec2(true, false)

bvec2 test_vec2_equal_function_all_true() {
    vec2 a = vec2(10.0, 20.0);
    vec2 b = vec2(10.0, 20.0);
    return equal(a, b);
}

// run: test_vec2_equal_function_all_true() == bvec2(true, true)

bvec2 test_vec2_equal_function_all_false() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(2.0, 4.0);
    return equal(a, b);
}

// run: test_vec2_equal_function_all_false() == bvec2(false, false)

bvec2 test_vec2_equal_function_mixed() {
    vec2 a = vec2(5.0, 3.0);
    vec2 b = vec2(2.0, 3.0);
    return equal(a, b);
}

// run: test_vec2_equal_function_mixed() == bvec2(false, true)

bvec2 test_vec2_equal_function_floats() {
    vec2 a = vec2(1.0, 2.0);
    vec2 b = vec2(1.0, 2.0);
    return equal(a, b);
}

// run: test_vec2_equal_function_floats() == bvec2(true, true)

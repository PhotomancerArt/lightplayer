// test run
// target riscv32.q32

// ============================================================================
// Forward Reference: Forward reference rules for global variables
// ============================================================================

// Global declared after functions that use it
float global_after_function;

float test_forward_reference_use_before_declare() {
    // Function uses global declared later
    return global_after_function + 10.0;
}

// run: test_forward_reference_use_before_declare() ~= 10.0 [expect-fail]

// Global declaration comes after function definition
float global_after_function = 42.0;

float test_forward_reference_after_declare() {
    // Function uses global after it's declared
    return global_after_function * 2.0;
}

// run: test_forward_reference_after_declare() ~= 84.0 [expect-fail]

// Multiple globals with forward references
vec2 global_vec_after;
mat3 global_mat_after;

vec2 test_forward_reference_vec() {
    // Function uses vec2 global declared later
    return global_vec_after + vec2(1.0, 1.0);
}

// run: test_forward_reference_vec() ~= vec2(1.0, 1.0) [expect-fail]

mat3 test_forward_reference_mat() {
    // Function uses mat3 global declared later
    return global_mat_after * 2.0;
}

// run: test_forward_reference_mat() ~= mat3(2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0) [expect-fail]

// Declarations after function definitions
vec2 global_vec_after = vec2(5.0, 10.0);
mat3 global_mat_after = mat3(1.0);

float test_forward_reference_complex() {
    // Complex forward reference usage
    float scalar_result = global_after_function;
    vec2 vec_result = global_vec_after;
    mat3 mat_result = global_mat_after;

    return scalar_result + vec_result.x + vec_result.y + mat_result[0][0];
}

// run: test_forward_reference_complex() ~= 58.0 [expect-fail]

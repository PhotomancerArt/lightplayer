// test run
// target riscv32.q32

// ============================================================================
// Redeclaration Error: Redeclaration of global variables in same scope is error
// ============================================================================

// Valid global declarations
float valid_global = 42.0;
int valid_int = 123;
vec2 valid_vec2 = vec2(1.0, 2.0);

// These would be compile errors (redeclaration in same scope):
// float valid_global = 24.0;    // Error: redeclaration of 'valid_global'
// int valid_int = 456;          // Error: redeclaration of 'valid_int'
// vec2 valid_vec2 = vec2(3.0, 4.0);  // Error: redeclaration of 'valid_vec2'

// However, we can declare different variables with different names
float different_global = 100.0;
int different_int = 200;
vec2 different_vec2 = vec2(5.0, 6.0);

float test_redeclare_error_valid() {
    // Test that valid declarations work
    return valid_global + different_global;
}

// run: test_redeclare_error_valid() ~= 142.0 [expect-fail]

int test_redeclare_error_different() {
    // Test different variable names work
    return valid_int + different_int;
}

// run: test_redeclare_error_different() == 323 [expect-fail]

vec2 test_redeclare_error_vecs() {
    // Test different vector variables
    return valid_vec2 + different_vec2;
}

// run: test_redeclare_error_vecs() ~= vec2(6.0, 8.0) [expect-fail]

float test_redeclare_error_scoping() {
    // Test that local variables can shadow globals (different scope)
    float valid_global = 99.0;  // This is allowed - shadows global
    return valid_global;  // Returns 99.0, not 42.0
}

// run: test_redeclare_error_scoping() ~= 99.0 [expect-fail]

float test_redeclare_error_global_unchanged() {
    // Verify global is unchanged after shadowing
    test_redeclare_error_scoping();
    return valid_global;  // Should still be 42.0
}

// run: test_redeclare_error_global_unchanged() ~= 42.0 [expect-fail]

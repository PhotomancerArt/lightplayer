// test run
// target riscv32.q32

// ============================================================================
// Local Variable Shadowing: Local variables can hide global variables
// ============================================================================

float global_counter = 100.0;
vec2 global_position = vec2(10.0, 20.0);
bool global_flag = true;
int global_value = 42;

float test_scope_shadowing_simple() {
    // Local variable shadows global
    float global_counter = 50.0;  // Shadows the global
    return global_counter;  // Returns 50.0, not 100.0
}

// run: test_scope_shadowing_simple() ~= 50.0 [expect-fail]

float test_scope_shadowing_verify_global() {
    // Verify global is unchanged after shadowing
    test_scope_shadowing_simple();
    return global_counter;  // Should still be 100.0
}

// run: test_scope_shadowing_verify_global() ~= 100.0 [expect-fail]

vec2 test_scope_shadowing_vector() {
    // Local vector shadows global vector
    vec2 global_position = vec2(1.0, 2.0);  // Shadows global
    return global_position;  // Returns (1.0, 2.0), not (10.0, 20.0)
}

// run: test_scope_shadowing_vector() ~= vec2(1.0, 2.0) [expect-fail]

vec2 test_scope_shadowing_verify_global_vector() {
    // Verify global vector is unchanged
    test_scope_shadowing_vector();
    return global_position;  // Should still be (10.0, 20.0)
}

// run: test_scope_shadowing_verify_global_vector() ~= vec2(10.0, 20.0) [expect-fail]

bool test_scope_shadowing_bool() {
    // Local bool shadows global bool
    bool global_flag = false;  // Shadows global
    return global_flag;  // Returns false, not true
}

// run: test_scope_shadowing_bool() == false [expect-fail]

bool test_scope_shadowing_verify_global_bool() {
    // Verify global bool is unchanged
    test_scope_shadowing_bool();
    return global_flag;  // Should still be true
}

// run: test_scope_shadowing_verify_global_bool() == true [expect-fail]

int test_scope_shadowing_int() {
    // Local int shadows global int
    int global_value = 99;  // Shadows global
    return global_value;  // Returns 99, not 42
}

// run: test_scope_shadowing_int() == 99 [expect-fail]

int test_scope_shadowing_verify_global_int() {
    // Verify global int is unchanged
    test_scope_shadowing_int();
    return global_value;  // Should still be 42
}

// run: test_scope_shadowing_verify_global_int() == 42 [expect-fail]

float test_scope_shadowing_in_function() {
    // Shadowing inside a function
    void test_func() {
        float global_counter = 123.0;  // Shadows global
        global_counter = global_counter * 2.0;  // Modifies local
    }

    test_func();
    return global_counter;  // Global should be unchanged
}

// run: test_scope_shadowing_in_function() ~= 100.0 [expect-fail]

float test_scope_shadowing_nested() {
    // Nested scopes with shadowing
    {
        float global_counter = 200.0;  // Shadows global
        {
            float global_counter = 300.0;  // Shadows outer local
            global_counter = global_counter + 50.0;  // Modifies innermost local
        }
        global_counter = global_counter + 25.0;  // Modifies outer local
    }

    return global_counter;  // Global should be unchanged
}

// run: test_scope_shadowing_nested() ~= 100.0 [expect-fail]

// test run
// target riscv32.q32

// ============================================================================
// Nested Scopes and Globals: Global variables accessible from nested scopes
// ============================================================================

float global_counter = 0.0;
vec2 global_position = vec2(0.0, 0.0);
bool global_flag = false;
int global_depth = 0;

float test_scope_nested_access() {
    // Global variables accessible from nested scopes
    {
        global_counter = global_counter + 1.0;
        {
            global_counter = global_counter + 2.0;
            {
                global_counter = global_counter + 3.0;
            }
        }
    }

    return global_counter;
}

// run: test_scope_nested_access() ~= 6.0 [expect-fail]

vec2 test_scope_nested_vector() {
    // Global vector accessible from nested scopes
    {
        global_position.x = 5.0;
        {
            global_position.y = 10.0;
            {
                global_position = global_position * 2.0;
            }
        }
    }

    return global_position;
}

// run: test_scope_nested_vector() ~= vec2(10.0, 20.0) [expect-fail]

bool test_scope_nested_bool() {
    // Global bool accessible from nested scopes
    {
        global_flag = true;
        {
            global_flag = !global_flag;  // becomes false
            {
                global_flag = !global_flag;  // becomes true
            }
        }
    }

    return global_flag;
}

// run: test_scope_nested_bool() == true [expect-fail]

int test_scope_nested_depth() {
    // Tracking nesting depth with global
    {
        global_depth = global_depth + 1;  // depth = 1
        {
            global_depth = global_depth + 1;  // depth = 2
            {
                global_depth = global_depth + 1;  // depth = 3
                {
                    global_depth = global_depth + 1;  // depth = 4
                }
                global_depth = global_depth - 1;  // depth = 3
            }
            global_depth = global_depth - 1;  // depth = 2
        }
        global_depth = global_depth - 1;  // depth = 1
    }
    global_depth = global_depth - 1;  // depth = 0

    return global_depth;
}

// run: test_scope_nested_depth() == 0 [expect-fail]

float test_scope_nested_mixed() {
    // Mixed local and global access in nested scopes
    float local_var = 10.0;

    {
        float local_var = 20.0;  // Shadows outer local
        global_counter = global_counter + local_var;  // Uses shadowed local

        {
            global_counter = global_counter + 5.0;  // Direct global access
            local_var = 30.0;  // Modifies inner local
            global_counter = global_counter + local_var;
        }
    }

    return global_counter + local_var;  // Uses outer local
}

// run: test_scope_nested_mixed() ~= 65.0 [expect-fail]

vec2 test_scope_nested_functions() {
    // Global access from nested function scopes
    void outer_func() {
        global_position.x = global_position.x + 1.0;

        void inner_func() {
            global_position.y = global_position.y + 1.0;

            void innermost_func() {
                global_position = global_position * 2.0;
            }

            innermost_func();
        }

        inner_func();
    }

    global_position = vec2(1.0, 1.0);
    outer_func();
    return global_position;
}

// run: test_scope_nested_functions() ~= vec2(4.0, 4.0) [expect-fail]

float test_scope_nested_complex() {
    // Complex nesting with multiple global accesses
    {
        global_counter = 1.0;
        {
            global_counter = global_counter * 2.0;  // 2.0
            {
                global_counter = global_counter + 3.0;  // 5.0
                {
                    float temp = global_counter;
                    global_counter = temp * 2.0;  // 10.0
                }
                global_counter = global_counter - 2.0;  // 8.0
            }
            global_counter = global_counter / 2.0;  // 4.0
        }
        global_counter = global_counter + 1.0;  // 5.0
    }

    return global_counter;
}

// run: test_scope_nested_complex() ~= 5.0 [expect-fail]

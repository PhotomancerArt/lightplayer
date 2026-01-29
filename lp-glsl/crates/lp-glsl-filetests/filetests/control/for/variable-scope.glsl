// test run
// target riscv32.q32

// ============================================================================
// Variable scoping in for loops (init-expression scope)
// Spec: Variables declared in init-expression are only in scope until the end
//       of the sub-statement of the for loop
// ============================================================================

int test_for_loop_init_scope() {
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        sum = sum + i;
    }
    // i is out of scope here
    return sum;
}

// run: test_for_loop_init_scope() == 3

int test_for_loop_init_shadowing() {
    int i = 100;
    int sum = 0;
    for (int i = 0; i < 3; i++) {
        sum = sum + i;
    }
    // Outer i should be unchanged (shadowed by loop variable)
    return i;
}

// run: test_for_loop_init_shadowing() == 100

int test_for_loop_condition_scope() {
    int sum = 0;
    for (int i = 0; i < 5; i++) {
        int j = i * 2;
        sum = sum + j;
    }
    return sum;
}

// run: test_for_loop_condition_scope() == 20

int test_for_loop_multiple_loops() {
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        sum = sum + i;
    }
    for (int i = 0; i < 3; i++) {
        sum = sum + i;
    }
    return sum;
}

// run: test_for_loop_multiple_loops() == 4

int test_for_loop_init_scope_after_loop() {
    int x = 0;
    for (int i = 0; i < 2; i++) {
        x = x + i;
    }
    // i is out of scope - should not be accessible
    return x;
}

// run: test_for_loop_init_scope_after_loop() == 1

int test_for_loop_loop_expression_scope() {
    int sum = 0;
    int i = 0;
    for (i = 0; i < 3; i++) {
        sum = sum + i;
    }
    // i should still be in scope (not declared in init-expression)
    return i;
}

// run: test_for_loop_loop_expression_scope() == 3

int test_for_loop_nested_same_name() {
    int sum = 0;
    for (int i = 0; i < 2; i++) {
        for (int i = 0; i < 3; i++) {
            sum = sum + 1;
        }
    }
    return sum;
}

// run: test_for_loop_nested_same_name() == 6

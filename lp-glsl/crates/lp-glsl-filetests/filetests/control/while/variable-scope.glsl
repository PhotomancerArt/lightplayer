// test run
// target riscv32.q32

// ============================================================================
// Variable scoping in while loops
// Spec: Variables declared in condition-expression are only in scope until
//       the end of the sub-statement of the while loop
// ============================================================================

int test_while_loop_variable_scope() {
    int sum = 0;
    int i = 0;
    while (i < 3) {
        int j = i * 2;
        sum = sum + j;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_variable_scope() == 6

int test_while_loop_shadowing() {
    int i = 100;
    int sum = 0;
    int j = 0;
    while (j < 3) {
        int i = j * 10;
        sum = sum + i;
        j = j + 1;
    }
    // Outer i should be unchanged
    return i;
}

// run: test_while_loop_shadowing() == 100

int test_while_loop_multiple_loops() {
    int sum = 0;
    int i = 0;
    while (i < 2) {
        sum = sum + i;
        i = i + 1;
    }
    i = 0;
    while (i < 3) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}

// run: test_while_loop_multiple_loops() == 4

int test_while_loop_condition_declaration() {
    int sum = 0;
    int i = 0;
    while (bool j = i < 3) {
        sum = sum + i;
        i = i + 1;
    }
    // j is out of scope here
    return sum;
}

// run: test_while_loop_condition_declaration() == 3

int test_while_loop_condition_scope() {
    int sum = 0;
    int i = 0;
    while (bool j = i < 3) {
        sum = sum + i;
        i = i + 1;
    }
    // j should be out of scope
    return sum;
}

// run: test_while_loop_condition_scope() == 3

int test_while_loop_condition_shadowing() {
    int i = 0;  // Outer i
    int j = 999;  // Outer j - should be shadowed by condition variable
    int sum = 0;
    // Variable j declared in condition shadows outer j
    // and is accessible within the loop body
    while (bool j = i < 3) {
        // Inner j should be accessible here and equal to (i < 3)
        // Inner j shadows outer j (999)
        // j is true when i < 3, false otherwise
        if (j) {
            sum = sum + i;
        }
        i = i + 1;
    }
    // Outer i should be modified (now 3)
    // Outer j should be unchanged (still 999)
    // Inner j should be out of scope here
    return j;  // Returns outer j to verify it wasn't affected
}

// test run

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

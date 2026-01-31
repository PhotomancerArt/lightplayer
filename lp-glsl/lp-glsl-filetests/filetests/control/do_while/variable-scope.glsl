// test run
// target riscv32.q32

// ============================================================================
// Variable scoping in do-while loops
// Spec: Do-while cannot declare a variable in its condition-expression
//       Variables declared in body are scoped to the body
// ============================================================================

int test_do_while_loop_variable_scope() {
    int sum = 0;
    int i = 0;
    do {
        int j = i * 2;
        sum = sum + j;
        i = i + 1;
    } while (i < 3);
    return sum;
}

// run: test_do_while_loop_variable_scope() == 6

int test_do_while_loop_body_scope() {
    int sum = 0;
    int i = 0;
    do {
        int j = 10;
        sum = sum + j;
        i = i + 1;
    } while (i < 2);
    // j is out of scope here
    return sum;
}

// run: test_do_while_loop_body_scope() == 20

int test_do_while_loop_shadowing() {
    int i = 100;
    int sum = 0;
    int j = 0;
    do {
        int i = j * 10;
        sum = sum + i;
        j = j + 1;
    } while (j < 3);
    // Outer i should be unchanged
    return i;
}

// run: test_do_while_loop_shadowing() == 100

int test_do_while_loop_condition_no_declaration() {
    int sum = 0;
    int i = 0;
    do {
        sum = sum + i;
        i = i + 1;
    } while (i < 3);
    // Condition uses existing variable, no declaration
    return sum;
}

// run: test_do_while_loop_condition_no_declaration() == 3


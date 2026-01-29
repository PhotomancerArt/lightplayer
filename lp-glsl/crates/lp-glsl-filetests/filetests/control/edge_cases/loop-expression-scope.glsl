// test run
// target riscv32.q32

// ============================================================================
// Loop expression variable modifications and scope
// Spec: Variables modified by loop-expression maintain their value after
//       the loop is exited, provided they are still in scope
// ============================================================================

int test_for_loop_expression_preserved() {
    int i = 0;
    for (i = 0; i < 3; i++) {
        // Loop body
    }
    return i;
}

// run: test_for_loop_expression_preserved() == 3

int test_for_loop_expression_modified_in_body() {
    int i = 0;
    for (i = 0; i < 5; i++) {
        i = i + 1; // Modify loop variable in body
    }
    return i;
}

// run: test_for_loop_expression_modified_in_body() == 5 [expect-fail]

int test_for_loop_expression_break() {
    int i = 0;
    for (i = 0; i < 10; i++) {
        if (i >= 5) {
            break;
        }
    }
    return i;
}

// run: test_for_loop_expression_break() == 5

int test_while_loop_variable_preserved() {
    int i = 0;
    while (i < 3) {
        i = i + 1;
    }
    return i;
}

// run: test_while_loop_variable_preserved() == 3

int test_do_while_loop_variable_preserved() {
    int i = 0;
    do {
        i = i + 1;
    } while (i < 3);
    return i;
}

// run: test_do_while_loop_variable_preserved() == 3

int test_for_loop_expression_continue() {
    int i = 0;
    for (i = 0; i < 5; i++) {
        if (i == 2) {
            continue;
        }
    }
    return i;
}

// run: test_for_loop_expression_continue() == 5






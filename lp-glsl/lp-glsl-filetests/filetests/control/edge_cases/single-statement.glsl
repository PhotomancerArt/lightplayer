// test run
// target riscv32.q32

// ============================================================================
// Single statements without braces
// ============================================================================

int test_if_single_statement() {
    int x = 0;
    if (true)
        x = 10;
    return x;
}

// run: test_if_single_statement() == 10

int test_if_else_single_statement() {
    int x = 0;
    if (false)
        x = 10;
    else
        x = 20;
    return x;
}

// run: test_if_else_single_statement() == 20

int test_for_loop_single_statement() {
    int sum = 0;
    for (int i = 0; i < 3; i++)
        sum = sum + i;
    return sum;
}

// run: test_for_loop_single_statement() == 3

int test_while_loop_single_statement() {
    int sum = 0;
    int i = 0;
    while (i < 3)
        sum = sum + (i = i + 1);
    return sum;
}

// run: test_while_loop_single_statement() == 6

int test_nested_single_statements() {
    int sum = 0;
    for (int i = 0; i < 2; i++)
        if (i == 0)
            sum = sum + 10;
        else
            sum = sum + 20;
    return sum;
}

// run: test_nested_single_statements() == 30






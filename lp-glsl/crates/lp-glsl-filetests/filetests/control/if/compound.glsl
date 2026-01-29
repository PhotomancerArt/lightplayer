// test run
// target riscv32.q32

// ============================================================================
// If statements with compound statements (braces)
// ============================================================================

int test_if_compound_single_statement() {
    int x = 0;
    if (true) {
        x = 10;
    }
    return x;
}

// run: test_if_compound_single_statement() == 10

int test_if_compound_multiple_statements() {
    int x = 0;
    int y = 0;
    if (true) {
        x = 5;
        y = 10;
    }
    return x + y;
}

// run: test_if_compound_multiple_statements() == 15

int test_if_compound_nested_blocks() {
    int x = 0;
    if (true) {
        int y = 5;
        {
            int z = 10;
            x = y + z;
        }
    }
    return x;
}

// run: test_if_compound_nested_blocks() == 15

int test_if_compound_false() {
    int x = 0;
    int y = 0;
    if (false) {
        x = 5;
        y = 10;
    }
    return x + y;
}

// run: test_if_compound_false() == 0






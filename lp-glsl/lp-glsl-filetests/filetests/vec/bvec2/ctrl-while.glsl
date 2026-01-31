// test run
// target riscv32.q32

// ============================================================================
// Control While: while (any(bvec2)) - loop condition
// ============================================================================

int test_bvec2_ctrl_while_any() {
    // Control flow conditions must be scalar bool, so use any() or all() to convert
    bvec2 condition = bvec2(true, false);
    int counter = 0;
    while (any(condition)) {
        counter = counter + 1;
        condition = bvec2(false, false); // Exit condition
    }
    return counter;
}

// run: test_bvec2_ctrl_while_any() == 1

int test_bvec2_ctrl_while_all() {
    bvec2 condition = bvec2(true, true);
    int counter = 0;
    while (all(condition)) {
        counter = counter + 1;
        condition = bvec2(true, false); // Exit condition
    }
    return counter;
}

// run: test_bvec2_ctrl_while_all() == 1

int test_bvec2_ctrl_while_false() {
    bvec2 condition = bvec2(false, false);
    int counter = 0;
    while (any(condition)) {
        counter = counter + 1;
    }
    return counter;
}

// run: test_bvec2_ctrl_while_false() == 0

int test_bvec2_ctrl_while_dynamic_condition() {
    bvec2 condition = bvec2(true, true);
    int counter = 0;
    while (any(condition)) {
        counter = counter + 1;
        condition = not(condition); // Flip condition each iteration
        if (counter > 5) break; // Prevent infinite loop in test
    }
    return counter;
}

// run: test_bvec2_ctrl_while_dynamic_condition() == 1

int test_bvec2_ctrl_while_with_operations() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    int counter = 0;
    while (any(equal(a, b))) {
        counter = counter + 1;
        a = b; // Make them equal to exit
    }
    return counter;
}

// run: test_bvec2_ctrl_while_with_operations() == 0

int test_bvec2_ctrl_while_complex_condition() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    int counter = 0;
    while (any(a) && any(b)) {
        counter = counter + 1;
        a.x = false; // Will make any(a) false
    }
    return counter;
}

// run: test_bvec2_ctrl_while_complex_condition() == 1

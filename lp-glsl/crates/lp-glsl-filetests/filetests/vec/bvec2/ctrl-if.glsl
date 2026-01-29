// test run
// target riscv32.q32

// ============================================================================
// Control If: if (any(bvec2)), if (all(bvec2)) - control flow with bvec2
// ============================================================================

bool test_bvec2_ctrl_if_any_true() {
    // Control flow conditions must be scalar bool, so use any() or all() to convert
    bvec2 condition = bvec2(true, false);
    bool result = false;
    if (any(condition)) {
        result = true;
    }
    return result;
}

// run: test_bvec2_ctrl_if_any_true() == true

bool test_bvec2_ctrl_if_any_false() {
    bvec2 condition = bvec2(false, false);
    bool result = false;
    if (any(condition)) {
        result = true;
    }
    return result;
}

// run: test_bvec2_ctrl_if_any_false() == false

bool test_bvec2_ctrl_if_all_true() {
    bvec2 condition = bvec2(true, true);
    bool result = false;
    if (all(condition)) {
        result = true;
    }
    return result;
}

// run: test_bvec2_ctrl_if_all_true() == true

bool test_bvec2_ctrl_if_all_false() {
    bvec2 condition = bvec2(true, false);
    bool result = false;
    if (all(condition)) {
        result = true;
    }
    return result;
}

// run: test_bvec2_ctrl_if_all_false() == false

bool test_bvec2_ctrl_if_else_any() {
    bvec2 condition = bvec2(true, false);
    bool result;
    if (any(condition)) {
        result = true;
    } else {
        result = false;
    }
    return result;
}

// run: test_bvec2_ctrl_if_else_any() == true

bool test_bvec2_ctrl_if_else_all() {
    bvec2 condition = bvec2(true, false);
    bool result;
    if (all(condition)) {
        result = true;
    } else {
        result = false;
    }
    return result;
}

// run: test_bvec2_ctrl_if_else_all() == false

bool test_bvec2_ctrl_if_nested() {
    bvec2 outer = bvec2(true, false);
    bvec2 inner = bvec2(false, true);
    bool result = false;
    if (any(outer)) {
        if (any(inner)) {
            result = true;
        }
    }
    return result;
}

// run: test_bvec2_ctrl_if_nested() == true

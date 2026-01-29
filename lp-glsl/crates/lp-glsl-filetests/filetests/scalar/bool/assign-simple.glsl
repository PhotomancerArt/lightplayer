// test run
// target riscv32.q32

// ============================================================================
// Simple assignment: bool = bool
// ============================================================================

bool test_bool_assignment_true() {
    bool a = true;
    bool b = false;
    b = a;
    return b;
}

// run: test_bool_assignment_true() == true

bool test_bool_assignment_false() {
    bool a = false;
    bool b = true;
    b = a;
    return b;
}

// run: test_bool_assignment_false() == false

bool test_bool_assignment_independence() {
    bool a = true;
    bool b = false;
    b = a;
    a = false;  // Modify a
    return b;
}

// run: test_bool_assignment_independence() == true

bool test_bool_assignment_self() {
    bool a = true;
    a = a;  // Self-assignment
    return a;
}

// run: test_bool_assignment_self() == true

bool test_bool_assignment_chain() {
    bool a = true;
    bool b = false;
    bool c = false;
    c = b = a;  // Chain assignment
    return c;
}

// run: test_bool_assignment_chain() == true

bool test_bool_assignment_from_expression() {
    bool a = true;
    bool b = false;
    bool c = false;
    c = a && b;
    return c;
}

// run: test_bool_assignment_from_expression() == false

bool test_bool_assignment_from_constructor() {
    bool a = false;
    a = bool(5);  // Assign from constructor
    return a;
}

// run: test_bool_assignment_from_constructor() == true

bool test_bool_assignment_multiple() {
    bool a = true;
    bool b = false;
    bool c = true;
    a = b;  // a becomes false
    b = c;  // b becomes true
    c = false;  // c becomes false
    // Verify all assignments: a=false, b=true, c=false
    return a == false && b == true && c == false;
}

// run: test_bool_assignment_multiple() == true


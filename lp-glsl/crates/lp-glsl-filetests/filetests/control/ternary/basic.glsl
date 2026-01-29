// test run
// target riscv32.q32

// ============================================================================
// Basic ternary operator tests
// Spec: ?: operates on three expressions (exp1 ? exp2 : exp3)
//       First expression must result in a scalar Boolean
//       Only one of exp2 and exp3 is evaluated
// ============================================================================

int test_ternary_bool_true() {
    bool b = true;
    return b ? 10 : 20;
}

// run: test_ternary_bool_true() == 10

int test_ternary_bool_false() {
    bool b = false;
    return b ? 10 : 20;
}

// run: test_ternary_bool_false() == 20

int test_ternary_bool_literal_true() {
    return true ? 5 : 15;
}

// run: test_ternary_bool_literal_true() == 5

int test_ternary_bool_literal_false() {
    return false ? 5 : 15;
}

// run: test_ternary_bool_literal_false() == 15

int test_ternary_bool_expression() {
    bool a = true;
    bool b = false;
    return (a && b) ? 100 : 200;
}

// run: test_ternary_bool_expression() == 200

int test_ternary_bool_not() {
    bool a = false;
    return !a ? 7 : 14;
}

// run: test_ternary_bool_not() == 7

int test_ternary_bool_comparison() {
    int x = 5;
    int y = 3;
    return (x > y) ? 30 : 40;
}

// run: test_ternary_bool_comparison() == 30

int test_ternary_bool_equality() {
    int x = 10;
    int y = 10;
    return (x == y) ? 50 : 60;
}

// run: test_ternary_bool_equality() == 50

int test_ternary_bool_inequality() {
    int x = 10;
    int y = 5;
    return (x != y) ? 70 : 80;
}

// run: test_ternary_bool_inequality() == 70

int test_ternary_bool_logical_and() {
    bool a = true;
    bool b = true;
    return (a && b) ? 90 : 100;
}

// run: test_ternary_bool_logical_and() == 90

int test_ternary_bool_logical_or() {
    bool a = false;
    bool b = true;
    return (a || b) ? 110 : 120;
}

// run: test_ternary_bool_logical_or() == 110

int test_ternary_assignment() {
    int x;
    x = true ? 5 : 10;
    return x;
}

// run: test_ternary_assignment() == 5

int test_ternary_in_expression() {
    int x = 3;
    int y = 4;
    return x + (true ? 10 : 20);
}

// run: test_ternary_in_expression() == 13

int test_ternary_as_function_argument() {
    return test_ternary_bool_true() + (false ? 1 : 2);
}

// run: test_ternary_as_function_argument() == 12

int test_ternary_in_return() {
    bool b = true;
    return b ? 100 : 200;
}

// run: test_ternary_in_return() == 100






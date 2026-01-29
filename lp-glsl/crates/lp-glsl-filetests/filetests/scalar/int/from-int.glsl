// test run
// target riscv32.q32

// ============================================================================
// Constructor: int(int) - identity constructor
// ============================================================================

int test_int_from_int_zero() {
    int i = 0;
    return int(i);
}

// run: test_int_from_int_zero() == 0

int test_int_from_int_positive() {
    int i = 42;
    return int(i);
}

// run: test_int_from_int_positive() == 42

int test_int_from_int_negative() {
    int i = -10;
    return int(i);
}

// run: test_int_from_int_negative() == -10

int test_int_from_int_literal_zero() {
    return int(0);
}

// run: test_int_from_int_literal_zero() == 0

int test_int_from_int_literal_positive() {
    return int(100);
}

// run: test_int_from_int_literal_positive() == 100

int test_int_from_int_literal_negative() {
    return int(-50);
}

// run: test_int_from_int_literal_negative() == -50

int test_int_from_int_expression() {
    int a = 5;
    int b = 3;
    return int(a - b);
}

// run: test_int_from_int_expression() == 2

int test_int_from_int_expression_negative() {
    int a = 3;
    int b = 5;
    return int(a - b);
}

// run: test_int_from_int_expression_negative() == -2

int test_int_from_int_nested() {
    int i = 42;
    return int(int(i));
}

// run: test_int_from_int_nested() == 42

int test_int_from_int_self() {
    int a = -100;
    int b = int(a);
    return b;
}

// run: test_int_from_int_self() == -100

int test_int_from_int_min() {
    int i = -2147483648;  // INT_MIN
    return int(i);
}

// run: test_int_from_int_min() == -2147483648

int test_int_from_int_max() {
    int i = 2147483647;  // INT_MAX
    return int(i);
}

// run: test_int_from_int_max() == 2147483647


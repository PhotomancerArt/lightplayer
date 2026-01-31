// test run
// target riscv32.q32

// ============================================================================
// Precedence and associativity tests
// Spec: Ternary operator (?:) has precedence 15 (low, but above assignment)
//       Associativity: Right to Left
// ============================================================================

int test_ternary_precedence_arithmetic() {
    bool b = true;
    // Ternary has lower precedence than arithmetic
    // Should be: (2 + 3) ? 10 : 20, but condition must be bool
    // Actually: b ? (2 + 3) : 20 = 5
    return b ? 2 + 3 : 20;
}

// run: test_ternary_precedence_arithmetic() == 5

int test_ternary_precedence_comparison() {
    bool b = true;
    // Comparison has higher precedence than ternary
    // Should be: b ? (5 > 3 ? 10 : 20) : 30
    // But: b ? ((5 > 3) ? 10 : 20) : 30 = b ? 10 : 30 = 10
    return b ? 5 > 3 ? 10 : 20 : 30;
}

// run: test_ternary_precedence_comparison() == 10

int test_ternary_precedence_logical() {
    bool a = true;
    bool b = false;
    // Logical AND has higher precedence than ternary
    // Should be: (a && b) ? 10 : 20 = false ? 10 : 20 = 20
    return a && b ? 10 : 20;
}

// run: test_ternary_precedence_logical() == 20

int test_ternary_precedence_parentheses() {
    bool b = true;
    // Parentheses override precedence
    return (b ? 5 : 10) + 3;
}

// run: test_ternary_precedence_parentheses() == 8

int test_ternary_associativity_right_to_left() {
    bool a = true;
    bool b = false;
    bool c = true;
    // Right-to-left associativity means:
    // a ? (b ? c ? 1 : 2 : 3) : 4
    // But actually, it groups as:
    // a ? (b ? (c ? 1 : 2) : 3) : 4
    // a=true -> first branch: (b ? (c ? 1 : 2) : 3)
    // b=false -> second branch: 3
    return a ? b ? c ? 1 : 2 : 3 : 4;
}

// run: test_ternary_associativity_right_to_left() == 3

int test_ternary_associativity_nested() {
    bool a = true;
    bool b = true;
    bool c = false;
    // Groups right-to-left:
    // a ? (b ? (c ? 1 : 2) : 3) : 4
    // a=true -> first branch: (b ? (c ? 1 : 2) : 3)
    // b=true -> first branch: (c ? 1 : 2)
    // c=false -> second branch: 2
    return a ? b ? c ? 1 : 2 : 3 : 4;
}

// run: test_ternary_associativity_nested() == 2

int test_ternary_precedence_vs_assignment() {
    bool b = true;
    int x;
    // Assignment has lower precedence than ternary
    // So: x = (b ? 10 : 20)
    x = b ? 10 : 20;
    return x;
}

// run: test_ternary_precedence_vs_assignment() == 10

int test_ternary_precedence_vs_arithmetic_assign() {
    bool b = false;
    int x = 5;
    // Ternary has higher precedence than +=
    // So: x += (b ? 10 : 20) = x += 20 = 25
    x += b ? 10 : 20;
    return x;
}

// run: test_ternary_precedence_vs_arithmetic_assign() == 25

int test_ternary_in_arithmetic_expression() {
    bool b = true;
    // Ternary in arithmetic: 2 * (b ? 3 : 4) = 2 * 3 = 6
    return 2 * (b ? 3 : 4);
}

// run: test_ternary_in_arithmetic_expression() == 6

int test_ternary_in_comparison() {
    bool b = true;
    // Ternary in comparison: (b ? 10 : 20) > 15 = 10 > 15 = false = 0
    return (b ? 10 : 20) > 15 ? 1 : 0;
}

// run: test_ternary_in_comparison() == 0

int test_ternary_chain_complex() {
    bool a = false;
    bool b = true;
    bool c = false;
    // Right-to-left: a ? (b ? (c ? 1 : 2) : 3) : 4
    // a=false -> second branch: 4
    return a ? b ? c ? 1 : 2 : 3 : 4;
}

// run: test_ternary_chain_complex() == 4






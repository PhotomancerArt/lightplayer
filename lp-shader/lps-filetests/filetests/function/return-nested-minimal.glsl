// compile-opt(inline.mode, never)

// test run

// ============================================================================
// Minimal nested if/else with returns - the pattern that's failing
// ============================================================================

float nested_if_else(int a, int b, bool flag) {
    if (a > b) {
        if (flag) {
            return float(a - b);
        } else {
            return float(a + b);
        }
    } else {
        return float(b - a);
    }
}

float test_nested_if_else() {
    return nested_if_else(5, 3, true);
}

// run: test_nested_if_else() ~= 2.0

// ============================================================================
// Even more minimal: just nested ifs, no outer else
// ============================================================================

float nested_if_only(int a, int b, bool flag) {
    if (a > b) {
        if (flag) {
            return float(a - b);
        } else {
            return float(a + b);
        }
    }
    return float(b - a);
}

float test_nested_if_only() {
    return nested_if_only(5, 3, true);
}

// run: test_nested_if_only() ~= 2.0

// ============================================================================
// Double nested (three levels)
// ============================================================================

float triple_nested(int x, bool a, bool b) {
    if (x > 0) {
        if (a) {
            if (b) {
                return 1.0;
            } else {
                return 2.0;
            }
        } else {
            return 3.0;
        }
    } else {
        return 4.0;
    }
}

float test_triple_nested() {
    return triple_nested(5, true, true);
}

// run: test_triple_nested() ~= 1.0

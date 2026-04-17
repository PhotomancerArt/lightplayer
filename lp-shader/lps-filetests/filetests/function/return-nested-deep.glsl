// compile-opt(inline.mode, never)

// ============================================================================
// Deeply Nested Return Tests: Return from various depths of nested ifs
// ============================================================================
// These tests exercise the WASM control flow stack balancing when returning
// from nested if statements. The emitter must correctly unwind the control
// stack to maintain WASM structural balance.

// ----------------------------------------------------------------------------
// Test 1: Return from 3-level nested if (the classic failing case)
// ----------------------------------------------------------------------------
float return_level3_if(float a, float b, float c) {
    if (a > 0.0) {
        if (b > 0.0) {
            if (c > 0.0) {
                return 3.0;  // Return from 3rd level
            }
            return 2.0;  // Return from 2nd level (else branch)
        }
        return 1.0;  // Return from 1st level (else branch)
    }
    return 0.0;  // Return from outer level
}

float test_return_from_3level_nested() {
    return return_level3_if(1.0, 1.0, 1.0);
}
// run: test_return_from_3level_nested() ~= 3.0

float test_return_from_3level_level2() {
    return return_level3_if(1.0, 1.0, -1.0);
}
// run: test_return_from_3level_level2() ~= 2.0

float test_return_from_3level_level1() {
    return return_level3_if(1.0, -1.0, 0.0);
}
// run: test_return_from_3level_level1() ~= 1.0

float test_return_from_3level_level0() {
    return return_level3_if(-1.0, 0.0, 0.0);
}
// run: test_return_from_3level_level0() ~= 0.0

// ----------------------------------------------------------------------------
// Test 2: Return from else branch of nested ifs
// ----------------------------------------------------------------------------
float return_from_else_nested(float x) {
    if (x > 0.0) {
        if (x > 10.0) {
            return 10.0;  // Then branch
        } else {
            return 5.0;   // Else branch of inner if
        }
    } else {
        return 0.0;  // Else branch of outer if
    }
}

float test_return_from_inner_else() {
    return return_from_else_nested(5.0);
}
// run: test_return_from_inner_else() ~= 5.0

float test_return_from_outer_else() {
    return return_from_else_nested(-5.0);
}
// run: test_return_from_outer_else() ~= 0.0

// ----------------------------------------------------------------------------
// Test 3: Multiple returns in sequence (all branches return)
// ----------------------------------------------------------------------------
int return_all_branches(int a, int b) {
    if (a > 0) {
        if (b > 0) {
            return 3;  // Both positive
        } else {
            return 2;  // A positive, B not
        }
    } else {
        if (b > 0) {
            return 1;  // A not, B positive
        } else {
            return 0;  // Neither positive
        }
    }
}

int test_return_all_positive() {
    return return_all_branches(1, 1);
}
// run: test_return_all_positive() == 3

int test_return_all_negative() {
    return return_all_branches(-1, -1);
}
// run: test_return_all_negative() == 0

// ----------------------------------------------------------------------------
// Test 4: Return early from void function with nested ifs
// ----------------------------------------------------------------------------
float void_return_result;

void void_return_early(float[4] arr) {
    if (arr[0] >= 0.0) {
        if (arr[1] >= 0.0) {
            if (arr[2] >= 0.0) {
                void_return_result = 3.0;
                return;  // Early return from void
            }
            void_return_result = 2.0;
            return;
        }
        void_return_result = 1.0;
        return;
    }
    void_return_result = 0.0;
    return;
}

float test_void_return_3level() {
    float data[4] = float[4](1.0, 2.0, 3.0, -4.0);
    void_return_early(data);
    return void_return_result;
}
// run: test_void_return_3level() ~= 3.0

// ----------------------------------------------------------------------------
// Test 5: Return with loop containing nested if
// ----------------------------------------------------------------------------
int return_in_loop_with_if(int[4] arr) {
    for (int i = 0; i < 4; i++) {
        if (arr[i] > 0) {
            if (arr[i] > 10) {
                return arr[i];  // Return from nested if inside loop
            }
        }
    }
    return -1;  // Not found
}

int test_return_in_loop_nested_if() {
    int data[4] = int[4](-1, 5, 15, 3);
    return return_in_loop_with_if(data);
}
// run: test_return_in_loop_nested_if() == 15

// ----------------------------------------------------------------------------
// Test 6: Complex nesting with if-else-if chain
// ----------------------------------------------------------------------------
float return_complex_chain(float x) {
    if (x > 100.0) {
        return 100.0;
    } else if (x > 50.0) {
        if (x > 75.0) {
            return 75.0;
        }
        return 50.0;
    } else if (x > 25.0) {
        return 25.0;
    } else {
        return 0.0;
    }
}

float test_return_complex_75() {
    return return_complex_chain(80.0);
}
// run: test_return_complex_75() ~= 75.0

float test_return_complex_50() {
    return return_complex_chain(60.0);
}
// run: test_return_complex_50() ~= 50.0

float test_return_complex_25() {
    return return_complex_chain(30.0);
}
// run: test_return_complex_25() ~= 25.0

// ----------------------------------------------------------------------------
// Test 7: Return from deeply nested (4 levels) if
// ----------------------------------------------------------------------------
float return_4level(float a, float b, float c, float d) {
    if (a > 0.0) {
        if (b > 0.0) {
            if (c > 0.0) {
                if (d > 0.0) {
                    return 4.0;
                }
                return 3.0;
            }
            return 2.0;
        }
        return 1.0;
    }
    return 0.0;
}

float test_return_4level_innermost() {
    return return_4level(1.0, 1.0, 1.0, 1.0);
}
// run: test_return_4level_innermost() ~= 4.0

float test_return_4level_level3() {
    return return_4level(1.0, 1.0, 1.0, -1.0);
}
// run: test_return_4level_level3() ~= 3.0

// ----------------------------------------------------------------------------
// Test 8: Return from middle of nested structure (not innermost or outermost)
// ----------------------------------------------------------------------------
float return_from_middle(float x, float y, float z) {
    if (x > 0.0) {
        if (y > 0.0) {
            return 2.0;  // Return from middle level
        }
        if (z > 0.0) {
            return 1.5;  // Different middle path
        }
        return 1.0;
    }
    return 0.0;
}

float test_return_from_middle_level() {
    return return_from_middle(1.0, 1.0, 0.0);
}
// run: test_return_from_middle_level() ~= 2.0

float test_return_from_middle_alt() {
    return return_from_middle(1.0, -1.0, 1.0);
}
// run: test_return_from_middle_alt() ~= 1.5

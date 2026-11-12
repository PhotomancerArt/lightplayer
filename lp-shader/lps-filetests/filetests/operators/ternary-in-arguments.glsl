// test run

// The ternary's last operand is an assignment-expression (GLSL ES 3.00
// grammar), so a comma after it ends the argument instead of joining a
// comma expression: `vec4(a, c ? 1.0 : 0.0, 0.0, 1.0)` has four arguments.

vec4 test_ternary_in_constructor() {
    bool c = true;
    return vec4(0.25, c ? 1.0 : 0.0, 0.5, 1.0);
}

// run: test_ternary_in_constructor() ~= vec4(0.25, 1.0, 0.5, 1.0)

float add2(float a, float b) {
    return a + b;
}

float test_ternary_in_call() {
    bool c = false;
    return add2(c ? 10.0 : 2.0, 3.0);
}

// run: test_ternary_in_call() ~= 5.0

float test_ternary_comma_in_middle() {
    // The middle operand is a full expression and may hold a comma.
    bool c = true;
    float x = c ? (1.0, 2.0) : 3.0;
    return x;
}

// run: test_ternary_comma_in_middle() ~= 2.0

float test_nested_ternary_in_call() {
    int i = 1;
    return add2(i == 0 ? 1.0 : i == 1 ? 2.0 : 4.0, 0.5);
}

// run: test_nested_ternary_in_call() ~= 2.5

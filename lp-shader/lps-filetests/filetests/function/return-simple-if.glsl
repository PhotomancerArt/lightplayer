// compile-opt(inline.mode, never)

// ============================================================================
// Simplest Early Return: The minimal case that fails
// ============================================================================

// This is the simplest pattern: return from if-then, followed by return after
float simple_if_return(float x) {
    if (x >= 0.0) {
        return x;
    }
    return -x;
}

float test_simple_if_return_positive() {
    return simple_if_return(5.0);
}
// run: test_simple_if_return_positive() ~= 5.0

float test_simple_if_return_negative() {
    return simple_if_return(-5.0);
}
// run: test_simple_if_return_negative() ~= 5.0

// ----------------------------------------------------------------------------
// Without else branch - similar but no return inside
// ----------------------------------------------------------------------------
float no_else_if_return(float x) {
    float result = 0.0;
    if (x >= 0.0) {
        result = x;
    }
    return result;
}

float test_no_else_if_return() {
    return no_else_if_return(5.0);
}
// run: test_no_else_if_return() ~= 5.0

// ----------------------------------------------------------------------------
// With explicit else branch
// ----------------------------------------------------------------------------
float with_else_return(float x) {
    if (x >= 0.0) {
        return x;
    } else {
        return -x;
    }
}

float test_with_else_return_positive() {
    return with_else_return(5.0);
}
// run: test_with_else_return_positive() ~= 5.0

float test_with_else_return_negative() {
    return with_else_return(-5.0);
}
// run: test_with_else_return_negative() ~= 5.0

// ----------------------------------------------------------------------------
// Both branches return, then code after
// ----------------------------------------------------------------------------
float both_return_then_code(float x) {
    float result;
    if (x >= 0.0) {
        result = x;
    } else {
        result = -x;
    }
    return result * 2.0;
}

float test_both_return_then_code() {
    return both_return_then_code(5.0);
}
// run: test_both_return_then_code() ~= 10.0

// ----------------------------------------------------------------------------
// Return from if-then, then more code, then return
// This is the pattern from return-early.glsl that fails
// ----------------------------------------------------------------------------
float return_then_code_then_return(float x) {
    if (x >= 0.0) {
        return x;  // Return from then
    }
    // Code after the if (executes when condition is false)
    float neg = -x;
    return neg;  // Return after if
}

float test_return_then_code_then_return_pos() {
    return return_then_code_then_return(5.0);
}
// run: test_return_then_code_then_return_pos() ~= 5.0

float test_return_then_code_then_return_neg() {
    return return_then_code_then_return(-5.0);
}
// run: test_return_then_code_then_return_neg() ~= 5.0

// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++x) - Integer
// ============================================================================

int test_preinc_scalar_int() {
    int x = 5;
    int result = ++x;  // Should increment x to 6, then return 6
    return result;     // Should return 6
}

// run: test_preinc_scalar_int() == 6

int test_preinc_scalar_int_zero() {
    int x = 0;
    int result = ++x;  // Should increment x to 1, then return 1
    return result;
}

// run: test_preinc_scalar_int_zero() == 1

int test_preinc_scalar_int_negative() {
    int x = -5;
    int result = ++x;  // Should increment x to -4, then return -4
    return result;
}

// run: test_preinc_scalar_int_negative() == -4

// ============================================================================
// Post-increment (x++) - Integer
// ============================================================================

int test_postinc_scalar_int() {
    int x = 5;
    int old_x = x++;
    return old_x + x;  // Should return 5 + 6 = 11
}

// run: test_postinc_scalar_int() == 11

int test_postinc_scalar_int_zero() {
    int x = 0;
    int old_x = x++;
    return old_x + x;  // Should return 0 + 1 = 1
}

// run: test_postinc_scalar_int_zero() == 1

int test_postinc_scalar_int_negative() {
    int x = -5;
    int old_x = x++;
    return old_x + x;  // Should return -5 + -4 = -9
}

// run: test_postinc_scalar_int_negative() == -9

// ============================================================================
// Pre-decrement (--x) - Integer
// ============================================================================

int test_predec_scalar_int() {
    int x = 8;
    int result = --x;  // Should decrement x to 7, then return 7
    return result;     // Should return 7
}

// run: test_predec_scalar_int() == 7

int test_predec_scalar_int_zero() {
    int x = 0;
    int result = --x;  // Should decrement x to -1, then return -1
    return result;
}

// run: test_predec_scalar_int_zero() == -1

int test_predec_scalar_int_negative() {
    int x = -5;
    int result = --x;  // Should decrement x to -6, then return -6
    return result;
}

// run: test_predec_scalar_int_negative() == -6

// ============================================================================
// Post-decrement (x--) - Integer
// ============================================================================

int test_postdec_scalar_int() {
    int x = 8;
    int old_x = x--;
    return old_x + x;  // Should return 8 + 7 = 15
}

// run: test_postdec_scalar_int() == 15

int test_postdec_scalar_int_zero() {
    int x = 0;
    int old_x = x--;
    return old_x + x;  // Should return 0 + -1 = -1
}

// run: test_postdec_scalar_int_zero() == -1

int test_postdec_scalar_int_negative() {
    int x = -5;
    int old_x = x--;
    return old_x + x;  // Should return -5 + -6 = -11
}

// run: test_postdec_scalar_int_negative() == -11

// ============================================================================
// Pre-increment (++x) - Float
// ============================================================================

float test_preinc_scalar_float() {
    float x = 3.5;
    float result = ++x;  // Should increment x to 4.5, then return 4.5
    return result;
}

// run: test_preinc_scalar_float() ~= 4.5

float test_preinc_scalar_float_zero() {
    float x = 0.0;
    float result = ++x;  // Should increment x to 1.0, then return 1.0
    return result;
}

// run: test_preinc_scalar_float_zero() ~= 1.0

float test_preinc_scalar_float_negative() {
    float x = -2.5;
    float result = ++x;  // Should increment x to -1.5, then return -1.5
    return result;
}

// run: test_preinc_scalar_float_negative() ~= -1.5

// ============================================================================
// Post-increment (x++) - Float
// ============================================================================

float test_postinc_scalar_float() {
    float x = 3.5;
    float old_x = x++;
    return old_x + x;  // Should be 3.5 + 4.5 = 8.0
}

// run: test_postinc_scalar_float() ~= 8.0

float test_postinc_scalar_float_zero() {
    float x = 0.0;
    float old_x = x++;
    return old_x + x;  // Should be 0.0 + 1.0 = 1.0
}

// run: test_postinc_scalar_float_zero() ~= 1.0

float test_postinc_scalar_float_negative() {
    float x = -2.5;
    float old_x = x++;
    return old_x + x;  // Should be -2.5 + -1.5 = -4.0
}

// run: test_postinc_scalar_float_negative() ~= -4.0

// ============================================================================
// Pre-decrement (--x) - Float
// ============================================================================

float test_predec_scalar_float() {
    float x = 5.5;
    float result = --x;  // Should decrement x to 4.5, then return 4.5
    return result;  // Should return 4.5
}

// run: test_predec_scalar_float() ~= 4.5

float test_predec_scalar_float_zero() {
    float x = 0.0;
    float result = --x;  // Should decrement x to -1.0, then return -1.0
    return result;
}

// run: test_predec_scalar_float_zero() ~= -1.0

float test_predec_scalar_float_negative() {
    float x = -2.5;
    float result = --x;  // Should decrement x to -3.5, then return -3.5
    return result;
}

// run: test_predec_scalar_float_negative() ~= -3.5

// ============================================================================
// Post-decrement (x--) - Float
// ============================================================================

float test_postdec_scalar_float() {
    float x = 5.2;
    float old_x = x--;
    return old_x + x;  // Should be 5.2 + 4.2 = 9.4
}

// run: test_postdec_scalar_float() ~= 9.4

float test_postdec_scalar_float_zero() {
    float x = 0.0;
    float old_x = x--;
    return old_x + x;  // Should be 0.0 + -1.0 = -1.0
}

// run: test_postdec_scalar_float_zero() ~= -1.0

float test_postdec_scalar_float_negative() {
    float x = -2.5;
    float old_x = x--;
    return old_x + x;  // Should be -2.5 + -3.5 = -6.0
}

// run: test_postdec_scalar_float_negative() ~= -6.0







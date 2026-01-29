// test run
// target riscv32.q32

// ============================================================================
// Type conversion tests
// Spec: Second and third expressions can be any type, including void,
//       as long as their types match, or there is an implicit conversion
//       that can be applied to make their types match
// ============================================================================

int test_ternary_int_both_branches() {
    bool b = true;
    return b ? 10 : 20;
}

// run: test_ternary_int_both_branches() == 10

float test_ternary_float_both_branches() {
    bool b = false;
    return b ? 1.5 : 2.5;
}

// run: test_ternary_float_both_branches() ~= 2.5

int test_ternary_int_to_float_conversion() {
    bool b = true;
    // int can be converted to float implicitly
    float result = b ? 5 : 10.0;
    return int(result);
}

// run: test_ternary_int_to_float_conversion() == 5

int test_ternary_float_to_int_conversion() {
    bool b = false;
    // float can be converted to int (truncated)
    int result = b ? 5.7 : 10.3;
    return result;
}

// run: test_ternary_float_to_int_conversion() == 10

int test_ternary_bool_to_int_conversion() {
    bool b = true;
    // bool can be converted to int (true=1, false=0)
    int result = b ? true : false;
    return result;
}

// run: test_ternary_bool_to_int_conversion() == 1

int test_ternary_int_to_bool_conversion() {
    bool b = false;
    // int can be converted to bool (non-zero=true, zero=false)
    bool result = b ? 1 : 0;
    return int(result);
}

// run: test_ternary_int_to_bool_conversion() == 0

int test_ternary_uint_to_int_conversion() {
    bool b = true;
    // uint can be converted to int
    uint u = 42u;
    int i = 24;
    int result = b ? int(u) : i;
    return result;
}

// run: test_ternary_uint_to_int_conversion() == 42

int test_ternary_mixed_numeric_types() {
    bool b = false;
    // Mixing int and float - should convert to float
    float result = b ? 5 : 10.5;
    return int(result * 2.0);
}

// run: test_ternary_mixed_numeric_types() == 21

int test_ternary_same_type_no_conversion() {
    bool b = true;
    int x = 100;
    int y = 200;
    return b ? x : y;
}

// run: test_ternary_same_type_no_conversion() == 100

float test_ternary_same_float_type() {
    bool b = false;
    float x = 1.5;
    float y = 2.5;
    return b ? x : y;
}

// run: test_ternary_same_float_type() ~= 2.5

bool test_ternary_same_bool_type() {
    bool b = true;
    bool x = true;
    bool y = false;
    return b ? x : y;
}

// run: test_ternary_same_bool_type() == true






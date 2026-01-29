// test run
// target riscv32.q32

// ============================================================================
// Scalar Global Types: Global variables of scalar types (float, int, uint, bool)
// ============================================================================

float global_float;
int global_int;
uint global_uint;
bool global_bool;

float test_type_scalar_float() {
    // Global float variable
    global_float = 42.5;
    return global_float;
}

// run: test_type_scalar_float() ~= 42.5 [expect-fail]

int test_type_scalar_int() {
    // Global int variable
    global_int = -123;
    return global_int;
}

// run: test_type_scalar_int() == -123 [expect-fail]

uint test_type_scalar_uint() {
    // Global uint variable
    global_uint = 987u;
    return int(global_uint);
}

// run: test_type_scalar_uint() == 987 [expect-fail]

bool test_type_scalar_bool() {
    // Global bool variable
    global_bool = true;
    return global_bool;
}

// run: test_type_scalar_bool() == true [expect-fail]

float test_type_scalar_float_operations() {
    // Float operations on global
    global_float = 10.0;
    global_float = global_float * 2.0;
    global_float = global_float + 5.0;
    return global_float;
}

// run: test_type_scalar_float_operations() ~= 25.0 [expect-fail]

int test_type_scalar_int_operations() {
    // Int operations on global
    global_int = 5;
    global_int = global_int * 3;
    global_int = global_int + 7;
    return global_int;
}

// run: test_type_scalar_int_operations() == 22 [expect-fail]

uint test_type_scalar_uint_operations() {
    // Uint operations on global
    global_uint = 10u;
    global_uint = global_uint * 2u;
    global_uint = global_uint + 5u;
    return int(global_uint);
}

// run: test_type_scalar_uint_operations() == 25 [expect-fail]

bool test_type_scalar_bool_operations() {
    // Bool operations on global
    global_bool = true;
    global_bool = !global_bool;  // false
    global_bool = !global_bool;  // true
    return global_bool;
}

// run: test_type_scalar_bool_operations() == true [expect-fail]

float test_type_scalar_mixed_operations() {
    // Mixed operations with different scalar globals
    global_float = 1.5;
    global_int = 2;
    global_uint = 3u;

    float result = global_float * float(global_int) + float(global_uint);
    return result;
}

// run: test_type_scalar_mixed_operations() ~= 6.0 [expect-fail]

bool test_type_scalar_comparison() {
    // Comparison operations with scalar globals
    global_float = 5.0;
    global_int = 5;
    global_uint = 5u;

    bool float_equal = global_float == 5.0;
    bool int_equal = global_int == 5;
    bool uint_equal = global_uint == 5u;

    return float_equal && int_equal && uint_equal;
}

// run: test_type_scalar_comparison() == true [expect-fail]

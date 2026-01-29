// test run
// target riscv32.q32

// ============================================================================
// Edge Const Write Error: Writing to const globals is a compile error
// ============================================================================

const float PI = 3.14159;
const int MAX_VALUE = 1000;
const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const mat2 IDENTITY = mat2(1.0, 0.0, 0.0, 1.0);

// These would be compile errors (cannot write to const):
// PI = 3.14;                    // Error: cannot assign to const
// MAX_VALUE = 2000;             // Error: cannot assign to const
// UNIT_VECTOR = vec2(0.0, 1.0); // Error: cannot assign to const
// IDENTITY = mat2(2.0);         // Error: cannot assign to const

// However, we can read from const variables
float test_edge_const_write_error_read() {
    // Reading from const is allowed
    return PI * 2.0;
}

// run: test_edge_const_write_error_read() ~= 6.28318 [expect-fail]

int test_edge_const_write_error_int() {
    // Reading const int is allowed
    return MAX_VALUE / 2;
}

// run: test_edge_const_write_error_int() == 500 [expect-fail]

vec2 test_edge_const_write_error_vec() {
    // Reading const vec2 is allowed
    return UNIT_VECTOR * 3.0;
}

// run: test_edge_const_write_error_vec() ~= vec2(3.0, 0.0) [expect-fail]

mat2 test_edge_const_write_error_mat() {
    // Reading const mat2 is allowed
    return IDENTITY * 2.0;
}

// run: test_edge_const_write_error_mat() ~= mat2(2.0, 0.0, 0.0, 2.0) [expect-fail]

float test_edge_const_write_error_calculations() {
    // Complex calculations using const values
    float radius = 5.0;
    float circumference = 2.0 * PI * radius;
    vec2 scaled_unit = UNIT_VECTOR * radius;

    return circumference + scaled_unit.x + scaled_unit.y;
}

// run: test_edge_const_write_error_calculations() ~= 36.28318 [expect-fail]

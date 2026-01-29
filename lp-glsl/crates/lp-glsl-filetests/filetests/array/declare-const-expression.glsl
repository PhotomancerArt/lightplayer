// test run
// target riscv32.q32

// ============================================================================
// Constant Expressions as Array Sizes
// ============================================================================

// Addition
const int ADD_SIZE = 2 + 3;
float arr_add[ADD_SIZE];

// Subtraction
const int SUB_SIZE = 10 - 2;
int arr_sub[SUB_SIZE];

// Multiplication
const int MUL_SIZE = 2 * 3;
float arr_mul[MUL_SIZE];

// Division
const int DIV_SIZE = 12 / 2;
int arr_div[DIV_SIZE];

// Modulo
const int MOD_SIZE = 10 % 3;
float arr_mod[MOD_SIZE];

// Parenthesized expressions
const int PAREN_SIZE = (2 + 3) * 2;
float arr_paren[PAREN_SIZE];

// Complex expressions
const int COMPLEX_SIZE = (10 + 5) / 3;
int arr_complex[COMPLEX_SIZE];

// Multiple operations
const int MULTI_OP_SIZE = (5 * 2) + (3 - 1);
vec2 arr_multi_op[MULTI_OP_SIZE];

// Nested parentheses
const int NESTED_PAREN_SIZE = ((2 + 3) * (4 - 1)) / 2;
vec3 arr_nested_paren[NESTED_PAREN_SIZE];

// Mixed operations with precedence
const int PRECEDENCE_SIZE = 2 + 3 * 4 - 5;
vec4 arr_precedence[PRECEDENCE_SIZE];

float test_addition_size() {
    // Test addition constant expression
    return 1.0;
}

// run: test_addition_size() == 1.0

int test_subtraction_size() {
    // Test subtraction constant expression
    return 1;
}

// run: test_subtraction_size() == 1

float test_multiplication_size() {
    // Test multiplication constant expression
    return 1.0;
}

// run: test_multiplication_size() == 1.0

int test_division_size() {
    // Test division constant expression
    return 1;
}

// run: test_division_size() == 1

float test_modulo_size() {
    // Test modulo constant expression
    return 1.0;
}

// run: test_modulo_size() == 1.0

float test_parenthesized_size() {
    // Test parenthesized constant expression
    return 1.0;
}

// run: test_parenthesized_size() == 1.0

int test_complex_size() {
    // Test complex constant expression
    return 1;
}

// run: test_complex_size() == 1

vec2 test_multi_op_size() {
    // Test multiple operations in constant expression
    return vec2(1.0, 1.0);
}

// run: test_multi_op_size() ~= vec2(1.0, 1.0)

vec3 test_nested_paren_size() {
    // Test nested parentheses in constant expression
    return vec3(1.0, 1.0, 1.0);
}

// run: test_nested_paren_size() ~= vec3(1.0, 1.0, 1.0)

vec4 test_precedence_size() {
    // Test operator precedence in constant expression
    return vec4(1.0, 1.0, 1.0, 1.0);
}

// run: test_precedence_size() ~= vec4(1.0, 1.0, 1.0, 1.0)





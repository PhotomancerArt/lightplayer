// test run
// target riscv32.q32

// ============================================================================
// Multiple Declarations: Multiple global variable declarations
// ============================================================================

// Multiple scalar globals
float global_a, global_b, global_c;
int int_x, int_y, int_z;
uint uint_p, uint_q, uint_r;
bool bool_flag1, bool_flag2, bool_flag3;

// Multiple vector globals
vec2 vec2_one, vec2_two, vec2_three;
vec3 vec3_alpha, vec3_beta, vec3_gamma;
vec4 vec4_red, vec4_green, vec4_blue;

// Multiple matrix globals
mat2 mat2_a, mat2_b;
mat3 mat3_x, mat3_y;
mat4 mat4_transform1, mat4_transform2;

// Mixed types in single declarations
float scalar1, scalar2;
vec2 vector1, vector2;
mat3 matrix1, matrix2;

float test_multiple_declare_scalars() {
    // Multiple scalar declarations
    global_a = 1.0;
    global_b = 2.0;
    global_c = 3.0;

    int_x = 10;
    int_y = 20;
    int_z = 30;

    return global_a + global_b + global_c + float(int_x + int_y + int_z);
}

// run: test_multiple_declare_scalars() ~= 66.0 [expect-fail]

vec2 test_multiple_declare_vectors() {
    // Multiple vector declarations
    vec2_one = vec2(1.0, 1.0);
    vec2_two = vec2(2.0, 2.0);
    vec2_three = vec2(3.0, 3.0);

    vec3_alpha = vec3(1.0, 0.0, 0.0);
    vec3_beta = vec3(0.0, 1.0, 0.0);
    vec3_gamma = vec3(0.0, 0.0, 1.0);

    return vec2_one + vec2_two + vec2_three;
}

// run: test_multiple_declare_vectors() ~= vec2(6.0, 6.0) [expect-fail]

mat2 test_multiple_declare_matrices() {
    // Multiple matrix declarations
    mat2_a = mat2(1.0, 2.0, 3.0, 4.0);
    mat2_b = mat2(2.0, 0.0, 0.0, 2.0);

    return mat2_a + mat2_b;
}

// run: test_multiple_declare_matrices() ~= mat2(3.0, 2.0, 3.0, 6.0) [expect-fail]

vec4 test_multiple_declare_colors() {
    // Multiple color declarations
    vec4_red = vec4(1.0, 0.0, 0.0, 1.0);
    vec4_green = vec4(0.0, 1.0, 0.0, 1.0);
    vec4_blue = vec4(0.0, 0.0, 1.0, 1.0);

    return vec4_red + vec4_green + vec4_blue;
}

// run: test_multiple_declare_colors() ~= vec4(1.0, 1.0, 1.0, 3.0) [expect-fail]

float test_multiple_declare_mixed() {
    // Mixed type declarations
    scalar1 = 5.0;
    scalar2 = 10.0;

    vector1 = vec2(1.0, 2.0);
    vector2 = vec2(3.0, 4.0);

    matrix1 = mat3(1.0);
    matrix2 = mat3(2.0);

    return scalar1 + scalar2 + vector1.x + vector1.y + vector2.x + vector2.y;
}

// run: test_multiple_declare_mixed() ~= 25.0 [expect-fail]

int test_multiple_declare_bools() {
    // Multiple boolean declarations
    bool_flag1 = true;
    bool_flag2 = false;
    bool_flag3 = true;

    uint_p = 100u;
    uint_q = 200u;
    uint_r = 300u;

    int result = 0;
    if (bool_flag1) result = result + 1;
    if (bool_flag2) result = result + 1;
    if (bool_flag3) result = result + 1;

    return result + int(uint_p + uint_q + uint_r);
}

// run: test_multiple_declare_bools() == 602 [expect-fail]

// test run
// target riscv32.q32

// ============================================================================
// Global Variable Access from Main: Accessing globals from main function
// ============================================================================

float global_counter = 0.0;
vec3 global_color = vec3(0.0, 0.0, 0.0);
bool global_enabled = false;
mat2 global_matrix = mat2(1.0);

float test_access_from_main_read() {
    // Main function reading global variables
    return global_counter + global_color.x + global_color.y + global_color.z;
}

// run: test_access_from_main_read() ~= 0.0 [expect-fail]

void test_access_from_main_write() {
    // Main function writing to global variables
    global_counter = 42.0;
    global_color = vec3(1.0, 0.5, 0.0);
    global_enabled = true;
    global_matrix = mat2(2.0, 0.0, 0.0, 2.0);
}

// run: test_access_from_main_write() == 0.0 [expect-fail]

float test_access_from_main_verify_write() {
    // Verify writes from main
    test_access_from_main_write();
    return global_counter + global_color.r + global_color.g + global_color.b;
}

// run: test_access_from_main_verify_write() ~= 43.5 [expect-fail]

bool test_access_from_main_flag() {
    // Main function manipulating boolean global
    global_enabled = false;
    global_enabled = !global_enabled;  // becomes true
    global_enabled = !global_enabled;  // becomes false
    return global_enabled;
}

// run: test_access_from_main_flag() == false [expect-fail]

mat2 test_access_from_main_matrix() {
    // Main function working with matrix global
    global_matrix = mat2(1.0, 2.0, 3.0, 4.0);
    global_matrix = global_matrix * 2.0;
    return global_matrix;
}

// run: test_access_from_main_matrix() ~= mat2(2.0, 4.0, 6.0, 8.0) [expect-fail]

float test_access_from_main_calculations() {
    // Main function with complex calculations using globals
    global_counter = 10.0;
    global_color = vec3(0.1, 0.2, 0.3);

    float total = global_counter;
    total = total + global_color.x * 10.0;
    total = total + global_color.y * 20.0;
    total = total + global_color.z * 30.0;

    global_counter = total;

    return global_counter;
}

// run: test_access_from_main_calculations() ~= 22.0 [expect-fail]

vec3 test_access_from_main_vector_ops() {
    // Main function with vector operations on globals
    global_color = vec3(0.5, 0.5, 0.5);
    global_color = global_color * 2.0;  // vec3(1.0, 1.0, 1.0)
    global_color = global_color + vec3(0.1, 0.2, 0.3);  // vec3(1.1, 1.2, 1.3)

    return global_color;
}

// run: test_access_from_main_vector_ops() ~= vec3(1.1, 1.2, 1.3) [expect-fail]

float test_access_from_main_mixed() {
    // Main function mixing different global types
    global_counter = 5.0;
    global_enabled = true;
    global_color = vec3(1.0, 0.0, 0.0);

    float result = global_counter;
    if (global_enabled) {
        result = result + global_color.x * 10.0;
    }
    result = result + global_color.y + global_color.z;

    return result;
}

// run: test_access_from_main_mixed() ~= 15.0 [expect-fail]

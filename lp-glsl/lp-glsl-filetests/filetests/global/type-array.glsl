// test run
// target riscv32.q32

// ============================================================================
// Array Global Types: Global variables of array types
// ============================================================================

float global_float_array[5];
vec2 global_vec2_array[3];
int global_int_array[4];
bool global_bool_array[3];

float test_type_array_float() {
    // Global float array
    global_float_array[0] = 1.0;
    global_float_array[1] = 2.0;
    global_float_array[2] = 3.0;
    global_float_array[3] = 4.0;
    global_float_array[4] = 5.0;

    return global_float_array[0] + global_float_array[1] + global_float_array[2] +
           global_float_array[3] + global_float_array[4];
}

// run: test_type_array_float() ~= 15.0 [expect-fail]

vec2 test_type_array_vec2() {
    // Global vec2 array
    global_vec2_array[0] = vec2(1.0, 2.0);
    global_vec2_array[1] = vec2(3.0, 4.0);
    global_vec2_array[2] = vec2(5.0, 6.0);

    return global_vec2_array[0] + global_vec2_array[1] + global_vec2_array[2];
}

// run: test_type_array_vec2() ~= vec2(9.0, 12.0) [expect-fail]

int test_type_array_int() {
    // Global int array
    global_int_array[0] = 10;
    global_int_array[1] = 20;
    global_int_array[2] = 30;
    global_int_array[3] = 40;

    return global_int_array[0] + global_int_array[1] + global_int_array[2] + global_int_array[3];
}

// run: test_type_array_int() == 100 [expect-fail]

bool test_type_array_bool() {
    // Global bool array
    global_bool_array[0] = true;
    global_bool_array[1] = false;
    global_bool_array[2] = true;

    return global_bool_array[0] && global_bool_array[2] && !global_bool_array[1];
}

// run: test_type_array_bool() == true [expect-fail]

float test_type_array_indexing() {
    // Array indexing operations
    global_float_array[0] = 100.0;
    global_float_array[4] = 200.0;

    int index = 2;
    global_float_array[index] = 50.0;

    return global_float_array[0] + global_float_array[index] + global_float_array[4];
}

// run: test_type_array_indexing() ~= 350.0 [expect-fail]

vec2 test_type_array_loop() {
    // Loop over array elements
    vec2 sum = vec2(0.0, 0.0);

    for (int i = 0; i < 3; i++) {
        global_vec2_array[i] = vec2(float(i + 1), float(i + 1) * 2.0);
        sum = sum + global_vec2_array[i];
    }

    return sum;
}

// run: test_type_array_loop() ~= vec2(6.0, 12.0) [expect-fail]

int test_type_array_length() {
    // Array length property
    global_int_array[0] = 1;
    global_int_array[1] = 2;
    global_int_array[2] = 3;
    global_int_array[3] = 4;

    return global_int_array.length();
}

// run: test_type_array_length() == 4 [expect-fail]

float test_type_array_multidimensional() {
    // Multi-dimensional array (array of arrays)
    vec2 array_of_arrays[2][3];

    array_of_arrays[0][0] = vec2(1.0, 2.0);
    array_of_arrays[0][1] = vec2(3.0, 4.0);
    array_of_arrays[0][2] = vec2(5.0, 6.0);
    array_of_arrays[1][0] = vec2(7.0, 8.0);
    array_of_arrays[1][1] = vec2(9.0, 10.0);
    array_of_arrays[1][2] = vec2(11.0, 12.0);

    return array_of_arrays[0][0].x + array_of_arrays[1][2].y;
}

// run: test_type_array_multidimensional() ~= 13.0 [expect-fail]

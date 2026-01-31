// test run
// target riscv32.q32

// ============================================================================
// Struct Global Types: Global variables of struct types
// ============================================================================

struct SimpleStruct {
    float x;
    int y;
};

struct VectorStruct {
    vec2 position;
    vec3 color;
    bool active;
};

struct NestedStruct {
    SimpleStruct simple;
    VectorStruct vector;
    float scale;
};

SimpleStruct global_simple_struct;
VectorStruct global_vector_struct;
NestedStruct global_nested_struct;

float test_type_struct_simple() {
    // Global simple struct
    global_simple_struct.x = 42.0;
    global_simple_struct.y = 123;

    return global_simple_struct.x + float(global_simple_struct.y);
}

// run: test_type_struct_simple() ~= 165.0 [expect-fail]

vec2 test_type_struct_vector() {
    // Global vector struct
    global_vector_struct.position = vec2(10.0, 20.0);
    global_vector_struct.color = vec3(1.0, 0.5, 0.0);
    global_vector_struct.active = true;

    return global_vector_struct.position;
}

// run: test_type_struct_vector() ~= vec2(10.0, 20.0) [expect-fail]

vec3 test_type_struct_vector_color() {
    // Access color from vector struct
    return global_vector_struct.color;
}

// run: test_type_struct_vector_color() ~= vec3(1.0, 0.5, 0.0) [expect-fail]

bool test_type_struct_vector_active() {
    // Access active flag from vector struct
    return global_vector_struct.active;
}

// run: test_type_struct_vector_active() == true [expect-fail]

float test_type_struct_nested() {
    // Global nested struct
    global_nested_struct.simple.x = 5.0;
    global_nested_struct.simple.y = 3;
    global_nested_struct.vector.position = vec2(1.0, 2.0);
    global_nested_struct.vector.color = vec3(0.8, 0.6, 0.4);
    global_nested_struct.vector.active = true;
    global_nested_struct.scale = 2.0;

    return global_nested_struct.simple.x * global_nested_struct.scale +
           global_nested_struct.vector.position.x;
}

// run: test_type_struct_nested() ~= 12.0 [expect-fail]

vec3 test_type_struct_nested_color() {
    // Access nested struct color
    return global_nested_struct.vector.color * global_nested_struct.scale;
}

// run: test_type_struct_nested_color() ~= vec3(1.6, 1.2, 0.8) [expect-fail]

int test_type_struct_nested_y() {
    // Access nested struct int member
    return global_nested_struct.simple.y * 2;
}

// run: test_type_struct_nested_y() == 6 [expect-fail]

float test_type_struct_operations() {
    // Struct member operations
    global_simple_struct.x = 10.0;
    global_simple_struct.y = 5;

    global_simple_struct.x = global_simple_struct.x * 2.0;
    global_simple_struct.y = global_simple_struct.y + 3;

    return global_simple_struct.x + float(global_simple_struct.y);
}

// run: test_type_struct_operations() ~= 28.0 [expect-fail]

vec2 test_type_struct_vector_operations() {
    // Vector struct operations
    global_vector_struct.position = vec2(1.0, 1.0);
    global_vector_struct.color = vec3(0.5, 0.5, 0.5);

    global_vector_struct.position = global_vector_struct.position * 3.0;
    global_vector_struct.color = global_vector_struct.color + vec3(0.2, 0.2, 0.2);

    return global_vector_struct.position + global_vector_struct.color.xy;
}

// run: test_type_struct_vector_operations() ~= vec2(3.7, 3.7) [expect-fail]

// test run
// target riscv32.q32

// ============================================================================
// Struct Fields with Const-Sized Arrays
// ============================================================================

const int FIELD_SIZE = 4;

struct TestStruct {
    float values[FIELD_SIZE];
    int counts[FIELD_SIZE];
};

// Struct with const-sized array fields
TestStruct test_struct_array() {
    TestStruct s;
    s.values[0] = 1.0;
    return s;
}

// Const expression in struct field
const int STRUCT_SIZE = 2 + 3;
struct ComplexStruct {
    vec2 data[STRUCT_SIZE];
};

// Multiple structs with different const sizes
const int SMALL_SIZE = 2;
const int MEDIUM_SIZE = 5;
struct MultiSizeStruct {
    vec3 small_array[SMALL_SIZE];
    vec4 medium_array[MEDIUM_SIZE];
};

// Struct with multi-dimensional array fields
const int MATRIX_ROWS = 3;
const int MATRIX_COLS = 3;
struct MatrixStruct {
    float matrices[MATRIX_ROWS][MATRIX_COLS];
};

// Struct containing other structs with arrays
struct NestedStruct {
    TestStruct inner;
    int extra_data;
};

// Test struct field access
float test_struct_field_access() {
    TestStruct s = test_struct_array();
    return s.values[0];
}

// run: test_struct_field_access() == 1.0 [expect-fail]

int test_struct_array_field() {
    TestStruct s = test_struct_array();
    return s.counts[0];
}

// run: test_struct_array_field() == 0 [expect-fail]

vec2 test_const_expr_struct() {
    ComplexStruct cs;
    cs.data[0] = vec2(1.0, 2.0);
    return cs.data[0];
}

// run: test_const_expr_struct() ~= vec2(1.0, 2.0) [expect-fail]

vec3 test_multi_size_struct() {
    MultiSizeStruct ms;
    ms.small_array[0] = vec3(1.0, 1.0, 1.0);
    return ms.small_array[0];
}

// run: test_multi_size_struct() ~= vec3(1.0, 1.0, 1.0) [expect-fail]

vec4 test_multi_size_medium() {
    MultiSizeStruct ms;
    ms.medium_array[0] = vec4(1.0, 1.0, 1.0, 1.0);
    return ms.medium_array[0];
}

// run: test_multi_size_medium() ~= vec4(1.0, 1.0, 1.0, 1.0) [expect-fail]

float test_matrix_struct() {
    MatrixStruct m;
    m.matrices[0][0] = 1.0;
    return m.matrices[0][0];
}

// run: test_matrix_struct() == 1.0 [expect-fail]

float test_nested_struct() {
    NestedStruct ns;
    ns.inner.values[0] = 2.0;
    return ns.inner.values[0];
}

// run: test_nested_struct() == 2.0 [expect-fail]

int test_nested_struct_extra() {
    NestedStruct ns;
    ns.extra_data = 42;
    return ns.extra_data;
}

// run: test_nested_struct_extra() == 42 [expect-fail]

// Struct with arrays of different types
const int MIXED_SIZE = 3;
struct MixedTypeStruct {
    float floats[MIXED_SIZE];
    int ints[MIXED_SIZE];
    vec2 vecs[MIXED_SIZE];
};

float test_mixed_type_struct() {
    MixedTypeStruct mts;
    mts.floats[0] = 1.0;
    mts.ints[0] = 2;
    mts.vecs[0] = vec2(3.0, 4.0);
    return mts.floats[0] + float(mts.ints[0]) + mts.vecs[0].x;
}

// run: test_mixed_type_struct() == 6.0 [expect-fail]





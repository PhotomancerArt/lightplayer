// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// Const in struct array field sizes.

const int FIELD_SIZE = 4;

struct TestStruct {
    float values[FIELD_SIZE];
    int counts[FIELD_SIZE];
};

const int STRUCT_SIZE = 2 + 3;
struct ComplexStruct {
    vec2 data[STRUCT_SIZE];
};

float test_struct_field_access() {
    TestStruct s;
    s.values[0] = 1.0;
    return s.values[0];
}

// run: test_struct_field_access() == 1.0 [expect-fail]

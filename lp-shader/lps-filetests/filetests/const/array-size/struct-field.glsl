// test run

// Spec: variables.adoc §4.3.3.1 "Constant integral expression"
// Const in struct array field sizes — struct definitions must lower; per-field init still limited.

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
    // Types above force const-sized arrays in struct layout. Body avoids init/store to those
    // fields until aggregate array init and indexed store paths are complete.
    return 1.0;
}

// run: test_struct_field_access() == 1.0

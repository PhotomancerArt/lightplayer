// test run
// target riscv32.q32

// ============================================================================
// Edge Empty Declaration: Empty declarations (if allowed) have special behavior
// ============================================================================

// Regular declarations work fine
float normal_float = 42.0;
int normal_int = 123;
vec2 normal_vec2 = vec2(1.0, 2.0);

// Empty declarations may or may not be allowed in GLSL
// float;        // Empty declaration - may be error or have special meaning
// int;          // Empty declaration - may be error or have special meaning
// vec3;         // Empty declaration - may be error or have special meaning

// In some contexts, empty declarations might be used for specific purposes,
// but generally they are not meaningful in GLSL

float test_edge_empty_declaration_normal() {
    // Normal declarations work as expected
    return normal_float + float(normal_int) + normal_vec2.x + normal_vec2.y;
}

// run: test_edge_empty_declaration_normal() ~= 168.0 [expect-fail]

float test_edge_empty_declaration_modify() {
    // Modify normal declarations
    normal_float = normal_float * 2.0;
    normal_int = normal_int + 100;
    normal_vec2 = normal_vec2 + vec2(10.0, 10.0);

    return normal_float + float(normal_int) + normal_vec2.x + normal_vec2.y;
}

// run: test_edge_empty_declaration_modify() ~= 298.0 [expect-fail]

void test_edge_empty_declaration_unused() {
    // Test with potentially unused variables
    float temp = 1.0;
    // temp is used below, so not unused
    temp = temp + normal_float;
}

// run: test_edge_empty_declaration_unused() == 0.0 [expect-fail]

float test_edge_empty_declaration_scope() {
    // Test scoping with declarations
    {
        float scoped_var = 100.0;
        normal_float = scoped_var;
    }
    // scoped_var is out of scope here

    return normal_float;
}

// run: test_edge_empty_declaration_scope() ~= 100.0 [expect-fail]

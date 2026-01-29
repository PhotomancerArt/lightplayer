// test run
// target riscv32.q32

// ============================================================================
// Struct Definitions with Nested Struct Members
// ============================================================================

struct Point {
    float x;
    float y;
};

struct Line {
    Point start;
    Point end;
};

float test_define_nested_line() {
    Line l; // Declaration test
    return 1.0; // Should be 1.0
}

// run: test_define_nested_line() == 1.0 [expect-fail]

struct Color {
    float r;
    float g;
    float b;
};

struct Material {
    Color diffuse;
    Color specular;
    float shininess;
};

int test_define_nested_material() {
    Material m; // Declaration test
    return 1; // Should be 1
}

// run: test_define_nested_material() == 1 [expect-fail]

struct Vector2D {
    float x;
    float y;
};

struct Vector3D {
    Vector2D xy;
    float z;
};

uint test_define_nested_vector3d() {
    Vector3D v; // Declaration test
    return 1u; // Should be 1u
}

// run: test_define_nested_vector3d() == 1u [expect-fail]

struct Person {
    int age;
    float height;
};

struct Family {
    Person father;
    Person mother;
    Person child;
};

bool test_define_nested_family() {
    Family f; // Declaration test
    return true; // Should be true
}

// run: test_define_nested_family() == true [expect-fail]

struct BoundingBox2D {
    Vector2D min;
    Vector2D max;
};

vec2 test_define_nested_bounding_box2d() {
    BoundingBox2D b; // Declaration test
    return vec2(1.0, 1.0); // Should be vec2(1.0, 1.0)
}

// run: test_define_nested_bounding_box2d() ~= vec2(1.0, 1.0) [expect-fail]

struct Node {
    int value;
    Node next; // This would be an error - can't have self-referencing struct
};

float test_define_nested_node() {
    // Node n; // This would be a compile error
    return 1.0; // We can't test the error, so just return success
}

// run: test_define_nested_node() == 1.0 [expect-fail]

struct TreeNode {
    int value;
    // Can't have direct self-reference, but can have pointers or references
};

int test_define_nested_tree_node() {
    TreeNode t; // Declaration test
    return 1; // Should be 1
}

// run: test_define_nested_tree_node() == 1 [expect-fail]

struct ComplexShape {
    Line outline;
    Color fillColor;
    float thickness;
};

uint test_define_nested_complex_shape() {
    ComplexShape s; // Declaration test
    return 1u; // Should be 1u
}

// run: test_define_nested_complex_shape() == 1u [expect-fail]

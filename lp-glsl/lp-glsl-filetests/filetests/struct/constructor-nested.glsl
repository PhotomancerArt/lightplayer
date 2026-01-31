// test run
// target riscv32.q32

// ============================================================================
// Struct Constructors with Nested Struct Members
// ============================================================================

struct Point {
    float x;
    float y;
};

struct Line {
    Point start;
    Point end;
};

float test_constructor_nested_line_start_x() {
    Line l = Line(Point(1.0, 2.0), Point(3.0, 4.0));
    return l.start.x; // Should be 1.0
}

// run: test_constructor_nested_line_start_x() ~= 1.0 [expect-fail]

float test_constructor_nested_line_end_y() {
    Line l = Line(Point(5.0, 6.0), Point(7.0, 8.0));
    return l.end.y; // Should be 8.0
}

// run: test_constructor_nested_line_end_y() ~= 8.0 [expect-fail]

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

float test_constructor_nested_material_diffuse_r() {
    Material m = Material(Color(0.1, 0.2, 0.3), Color(0.8, 0.9, 0.7), 32.0);
    return m.diffuse.r; // Should be 0.1
}

// run: test_constructor_nested_material_diffuse_r() ~= 0.1 [expect-fail]

float test_constructor_nested_material_specular_g() {
    Material m = Material(Color(0.5, 0.6, 0.7), Color(0.2, 0.3, 0.4), 64.0);
    return m.specular.g; // Should be 0.3
}

// run: test_constructor_nested_material_specular_g() ~= 0.3 [expect-fail]

float test_constructor_nested_material_shininess() {
    Material m = Material(Color(1.0, 1.0, 1.0), Color(0.5, 0.5, 0.5), 128.0);
    return m.shininess; // Should be 128.0
}

// run: test_constructor_nested_material_shininess() ~= 128.0 [expect-fail]

struct Vector2D {
    float x;
    float y;
};

struct Vector3D {
    Vector2D xy;
    float z;
};

float test_constructor_nested_vector3d_xy_x() {
    Vector3D v = Vector3D(Vector2D(1.0, 2.0), 3.0);
    return v.xy.x; // Should be 1.0
}

// run: test_constructor_nested_vector3d_xy_x() ~= 1.0 [expect-fail]

float test_constructor_nested_vector3d_z() {
    Vector3D v = Vector3D(Vector2D(4.0, 5.0), 6.0);
    return v.z; // Should be 6.0
}

// run: test_constructor_nested_vector3d_z() ~= 6.0 [expect-fail]

struct Person {
    int age;
    float height;
};

struct Family {
    Person father;
    Person mother;
    Person child;
};

int test_constructor_nested_family_father_age() {
    Family f = Family(Person(45, 180.0), Person(42, 165.0), Person(10, 120.0));
    return f.father.age; // Should be 45
}

// run: test_constructor_nested_family_father_age() == 45 [expect-fail]

float test_constructor_nested_family_mother_height() {
    Family f = Family(Person(50, 175.0), Person(48, 170.0), Person(15, 150.0));
    return f.mother.height; // Should be 170.0
}

// run: test_constructor_nested_family_mother_height() ~= 170.0 [expect-fail]

int test_constructor_nested_family_child_age() {
    Family f = Family(Person(40, 185.0), Person(38, 168.0), Person(8, 110.0));
    return f.child.age; // Should be 8
}

// run: test_constructor_nested_family_child_age() == 8 [expect-fail]

struct BoundingBox2D {
    Vector2D min;
    Vector2D max;
};

float test_constructor_nested_bounding_box_min_x() {
    BoundingBox2D b = BoundingBox2D(Vector2D(0.0, 0.0), Vector2D(10.0, 10.0));
    return b.min.x; // Should be 0.0
}

// run: test_constructor_nested_bounding_box_min_x() ~= 0.0 [expect-fail]

float test_constructor_nested_bounding_box_max_y() {
    BoundingBox2D b = BoundingBox2D(Vector2D(5.0, 5.0), Vector2D(15.0, 15.0));
    return b.max.y; // Should be 15.0
}

// run: test_constructor_nested_bounding_box_max_y() ~= 15.0 [expect-fail]

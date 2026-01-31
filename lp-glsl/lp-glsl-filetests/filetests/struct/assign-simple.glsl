// test run
// target riscv32.q32

// ============================================================================
// Simple Whole Struct Assignment
// ============================================================================

struct Point {
    float x;
    float y;
};

float test_assign_simple_point() {
    Point p1 = Point(1.0, 2.0);
    Point p2 = Point(3.0, 4.0);
    p1 = p2; // Whole struct assignment
    return p1.x; // Should be 3.0
}

// run: test_assign_simple_point() ~= 3.0 [expect-fail]

float test_assign_simple_point_y() {
    Point p1 = Point(5.0, 6.0);
    Point p2 = Point(7.0, 8.0);
    p1 = p2; // Whole struct assignment
    return p1.y; // Should be 8.0
}

// run: test_assign_simple_point_y() ~= 8.0 [expect-fail]

struct Color {
    float r;
    float g;
    float b;
};

float test_assign_simple_color() {
    Color c1 = Color(0.1, 0.2, 0.3);
    Color c2 = Color(0.4, 0.5, 0.6);
    c1 = c2; // Whole struct assignment
    return c1.g; // Should be 0.5
}

// run: test_assign_simple_color() ~= 0.5 [expect-fail]

struct Transform {
    vec3 position;
    vec3 rotation;
};

vec3 test_assign_simple_transform_position() {
    Transform t1 = Transform(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0));
    Transform t2 = Transform(vec3(7.0, 8.0, 9.0), vec3(10.0, 11.0, 12.0));
    t1 = t2; // Whole struct assignment
    return t1.position; // Should be vec3(7.0, 8.0, 9.0)
}

// run: test_assign_simple_transform_position() ~= vec3(7.0, 8.0, 9.0) [expect-fail]

vec3 test_assign_simple_transform_rotation() {
    Transform t1 = Transform(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0));
    Transform t2 = Transform(vec3(1.0, 1.0, 1.0), vec3(0.1, 0.2, 0.3));
    t1 = t2; // Whole struct assignment
    return t1.rotation; // Should be vec3(0.1, 0.2, 0.3)
}

// run: test_assign_simple_transform_rotation() ~= vec3(0.1, 0.2, 0.3) [expect-fail]

struct Person {
    int age;
    float height;
    bool isStudent;
};

int test_assign_simple_person_age() {
    Person p1 = Person(25, 175.0, true);
    Person p2 = Person(30, 180.0, false);
    p1 = p2; // Whole struct assignment
    return p1.age; // Should be 30
}

// run: test_assign_simple_person_age() == 30 [expect-fail]

float test_assign_simple_person_height() {
    Person p1 = Person(20, 165.0, true);
    Person p2 = Person(35, 190.0, false);
    p1 = p2; // Whole struct assignment
    return p1.height; // Should be 190.0
}

// run: test_assign_simple_person_height() ~= 190.0 [expect-fail]

bool test_assign_simple_person_student() {
    Person p1 = Person(22, 170.0, true);
    Person p2 = Person(28, 175.0, false);
    p1 = p2; // Whole struct assignment
    return p1.isStudent; // Should be false
}

// run: test_assign_simple_person_student() == false [expect-fail]

struct Line {
    Point start;
    Point end;
};

float test_assign_simple_line_nested() {
    Line l1 = Line(Point(1.0, 2.0), Point(3.0, 4.0));
    Line l2 = Line(Point(5.0, 6.0), Point(7.0, 8.0));
    l1 = l2; // Whole struct assignment (with nested structs)
    return l1.start.x; // Should be 5.0
}

// run: test_assign_simple_line_nested() ~= 5.0 [expect-fail]

float test_assign_simple_line_nested_end_y() {
    Line l1 = Line(Point(0.0, 0.0), Point(1.0, 1.0));
    Line l2 = Line(Point(10.0, 20.0), Point(30.0, 40.0));
    l1 = l2; // Whole struct assignment
    return l1.end.y; // Should be 40.0
}

// run: test_assign_simple_line_nested_end_y() ~= 40.0 [expect-fail]

struct EmptyData {
    int id;
};

int test_assign_simple_empty_data() {
    EmptyData d1 = EmptyData(42);
    EmptyData d2 = EmptyData(99);
    d1 = d2; // Whole struct assignment
    return d1.id; // Should be 99
}

// run: test_assign_simple_empty_data() == 99 [expect-fail]

// test run
// target riscv32.q32

// ============================================================================
// Simple Struct Constructors (scalar members)
// ============================================================================

struct Point {
    float x;
    float y;
};

float test_constructor_simple_point() {
    Point p = Point(1.0, 2.0);
    return p.x; // Should be 1.0
}

// run: test_constructor_simple_point() ~= 1.0 [expect-fail]

int test_constructor_simple_point_y() {
    Point p = Point(3.0, 4.0);
    return int(p.y); // Should be 4
}

// run: test_constructor_simple_point_y() == 4 [expect-fail]

struct Color {
    float r;
    float g;
    float b;
};

float test_constructor_simple_color() {
    Color c = Color(0.5, 0.7, 1.0);
    return c.g; // Should be 0.7
}

// run: test_constructor_simple_color() ~= 0.7 [expect-fail]

struct Triangle {
    float a;
    float b;
    float c;
};

float test_constructor_simple_triangle() {
    Triangle t = Triangle(3.0, 4.0, 5.0);
    return t.a + t.b + t.c; // 3.0 + 4.0 + 5.0 = 12.0
}

// run: test_constructor_simple_triangle() ~= 12.0 [expect-fail]

struct Person {
    int age;
    float height;
    bool isStudent;
};

int test_constructor_simple_person() {
    Person p = Person(25, 175.5, true);
    return p.age; // Should be 25
}

// run: test_constructor_simple_person() == 25 [expect-fail]

float test_constructor_simple_person_height() {
    Person p = Person(30, 180.0, false);
    return p.height; // Should be 180.0
}

// run: test_constructor_simple_person_height() ~= 180.0 [expect-fail]

bool test_constructor_simple_person_student() {
    Person p = Person(20, 165.0, true);
    return p.isStudent; // Should be true
}

// run: test_constructor_simple_person_student() == true [expect-fail]

struct Circle {
    float radius;
};

float test_constructor_simple_circle() {
    Circle c = Circle(10.0);
    return c.radius; // Should be 10.0
}

// run: test_constructor_simple_circle() ~= 10.0 [expect-fail]

struct EmptyData {
    int id;
};

int test_constructor_simple_empty_data() {
    EmptyData d = EmptyData(42);
    return d.id; // Should be 42
}

// run: test_constructor_simple_empty_data() == 42 [expect-fail]

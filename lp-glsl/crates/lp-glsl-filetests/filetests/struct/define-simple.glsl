// test run
// target riscv32.q32

// ============================================================================
// Simple Struct Definitions (scalar members)
// ============================================================================

struct Point {
    float x;
    float y;
};

float test_define_simple_point() {
    Point p; // Declaration test
    return 1.0; // Should be 1.0 (test passes if no compilation error)
}

// run: test_define_simple_point() == 1.0 [expect-fail]

struct Color {
    float r;
    float g;
    float b;
};

int test_define_simple_color() {
    Color c; // Declaration test
    return 1; // Should be 1
}

// run: test_define_simple_color() == 1 [expect-fail]

struct Triangle {
    float a;
    float b;
    float c;
};

uint test_define_simple_triangle() {
    Triangle t; // Declaration test
    return 1u; // Should be 1u
}

// run: test_define_simple_triangle() == 1u [expect-fail]

struct Person {
    int age;
    float height;
    bool isStudent;
};

bool test_define_simple_person() {
    Person p; // Declaration test
    return true; // Should be true
}

// run: test_define_simple_person() == true [expect-fail]

struct Vector2D {
    float x;
    float y;
};

vec2 test_define_simple_vector2d() {
    Vector2D v; // Declaration test
    return vec2(1.0, 1.0); // Should be vec2(1.0, 1.0)
}

// run: test_define_simple_vector2d() ~= vec2(1.0, 1.0) [expect-fail]

struct Circle {
    float radius;
};

float test_define_simple_circle() {
    Circle c; // Declaration test
    return 1.0; // Should be 1.0
}

// run: test_define_simple_circle() == 1.0 [expect-fail]

struct EmptyData {
    int id;
};

int test_define_simple_empty_data() {
    EmptyData d; // Declaration test (struct with single member)
    return 1; // Should be 1
}

// run: test_define_simple_empty_data() == 1 [expect-fail]

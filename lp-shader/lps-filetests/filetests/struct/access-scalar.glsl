// test run

// ============================================================================
// Scalar Member Access
// ============================================================================

struct Point {
    float x;
    float y;
};

float test_access_scalar_point_x() {
    Point p = Point(1.0, 2.0);
    return p.x; // Access x member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_point_x() ~= 1.0

float test_access_scalar_point_y() {
    Point p = Point(3.0, 4.0);
    return p.y; // Access y member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_point_y() ~= 4.0

struct Color {
    float r;
    float g;
    float b;
};

float test_access_scalar_color_r() {
    Color c = Color(0.1, 0.2, 0.3);
    return c.r; // Access r member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_color_r() ~= 0.1

float test_access_scalar_color_g() {
    Color c = Color(0.5, 0.7, 0.9);
    return c.g; // Access g member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_color_g() ~= 0.7

float test_access_scalar_color_b() {
    Color c = Color(1.0, 0.5, 0.0);
    return c.b; // Access b member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_color_b() ~= 0.0

struct Triangle {
    float a;
    float b;
    float c;
};

float test_access_scalar_triangle_a() {
    Triangle t = Triangle(3.0, 4.0, 5.0);
    return t.a; // Access a member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_triangle_a() ~= 3.0

float test_access_scalar_triangle_sum() {
    Triangle t = Triangle(1.0, 2.0, 3.0);
    return t.a + t.b + t.c; // Access all members
}

// @unimplemented(jit.q32)
// run: test_access_scalar_triangle_sum() ~= 6.0

struct Person {
    int age;
    float height;
    bool isStudent;
};

int test_access_scalar_person_age() {
    Person p = Person(25, 175.5, true);
    return p.age; // Access age member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_person_age() == 25

float test_access_scalar_person_height() {
    Person p = Person(30, 180.0, false);
    return p.height; // Access height member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_person_height() ~= 180.0

bool test_access_scalar_person_is_student() {
    Person p = Person(20, 165.0, true);
    return p.isStudent; // Access isStudent member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_person_is_student() == true

struct Circle {
    float radius;
};

float test_access_scalar_circle_radius() {
    Circle c = Circle(10.0);
    return c.radius; // Access radius member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_circle_radius() ~= 10.0

struct EmptyData {
    int id;
};

int test_access_scalar_empty_data_id() {
    EmptyData d = EmptyData(42);
    return d.id; // Access id member
}

// @unimplemented(jit.q32)
// run: test_access_scalar_empty_data_id() == 42

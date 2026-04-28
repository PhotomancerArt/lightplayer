// test run

struct Point {
    float x;
    float y;
};

struct Color {
    float r;
    float g;
    float b;
};

struct Person {
    int age;
    float height;
    bool isStudent;
};

struct Counter {
    uint count;
};

// === Declaration ===

float test_basic_declare_point() {
    Point p;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_basic_declare_point() == 1.0

int test_basic_declare_color() {
    Color c;
    return 1;
}

// @unimplemented(jit.q32)
// run: test_basic_declare_color() == 1

bool test_basic_declare_person() {
    Person p;
    return true;
}

// @unimplemented(jit.q32)
// run: test_basic_declare_person() == true

uint test_basic_declare_counter() {
    Counter c;
    return 1u;
}

// @unimplemented(jit.q32)
// run: test_basic_declare_counter() == 1u

// === Construction ===

float test_basic_construct_point_x() {
    Point p = Point(1.0, 2.0);
    return p.x;
}

// @unimplemented(jit.q32)
// run: test_basic_construct_point_x() ~= 1.0

float test_basic_construct_point_y() {
    Point p = Point(1.0, 2.0);
    return p.y;
}

// @unimplemented(jit.q32)
// run: test_basic_construct_point_y() ~= 2.0

float test_basic_construct_color_g() {
    Color c = Color(0.1, 0.7, 0.3);
    return c.g;
}

// @unimplemented(jit.q32)
// run: test_basic_construct_color_g() ~= 0.7

float test_basic_construct_color_b() {
    Color c = Color(0.1, 0.2, 0.9);
    return c.b;
}

// @unimplemented(jit.q32)
// run: test_basic_construct_color_b() ~= 0.9

int test_basic_construct_person_age() {
    Person p = Person(25, 175.5, true);
    return p.age;
}

// @unimplemented(jit.q32)
// run: test_basic_construct_person_age() == 25

uint test_basic_construct_counter() {
    Counter c = Counter(42u);
    return c.count;
}

// @unimplemented(jit.q32)
// run: test_basic_construct_counter() == 42u

// === Member read ===

float test_basic_read_point_x() {
    Point p = Point(3.0, 4.0);
    return p.x;
}

// @unimplemented(jit.q32)
// run: test_basic_read_point_x() ~= 3.0

float test_basic_read_color_r() {
    Color c = Color(0.5, 0.0, 0.0);
    return c.r;
}

// @unimplemented(jit.q32)
// run: test_basic_read_color_r() ~= 0.5

bool test_basic_read_person_is_student() {
    Person p = Person(30, 180.0, false);
    return p.isStudent;
}

// @unimplemented(jit.q32)
// run: test_basic_read_person_is_student() == false

float test_basic_read_person_height() {
    Person p = Person(20, 190.0, true);
    return p.height;
}

// @unimplemented(jit.q32)
// run: test_basic_read_person_height() ~= 190.0

// === Member write ===

float test_basic_write_point_x() {
    Point p = Point(0.0, 0.0);
    p.x = 5.0;
    return p.x;
}

// @unimplemented(jit.q32)
// run: test_basic_write_point_x() ~= 5.0

float test_basic_write_color_g() {
    Color c = Color(0.0, 0.0, 0.0);
    c.g = 0.8;
    return c.g;
}

// @unimplemented(jit.q32)
// run: test_basic_write_color_g() ~= 0.8

int test_basic_write_person_age() {
    Person p = Person(0, 0.0, false);
    p.age = 7;
    return p.age;
}

// @unimplemented(jit.q32)
// run: test_basic_write_person_age() == 7

uint test_basic_write_counter() {
    Counter c = Counter(0u);
    c.count = 99u;
    return c.count;
}

// @unimplemented(jit.q32)
// run: test_basic_write_counter() == 99u

// === Whole-struct assignment ===

float test_basic_assign_point_x() {
    Point p1 = Point(1.0, 2.0);
    Point p2 = Point(3.0, 4.0);
    p1 = p2;
    return p1.x;
}

// @unimplemented(jit.q32)
// run: test_basic_assign_point_x() ~= 3.0

float test_basic_assign_color_g() {
    Color c1 = Color(0.1, 0.2, 0.3);
    Color c2 = Color(0.4, 0.5, 0.6);
    c1 = c2;
    return c1.g;
}

// @unimplemented(jit.q32)
// run: test_basic_assign_color_g() ~= 0.5

int test_basic_assign_person_age() {
    Person a = Person(1, 1.0, false);
    Person b = Person(50, 2.0, true);
    a = b;
    return a.age;
}

// @unimplemented(jit.q32)
// run: test_basic_assign_person_age() == 50

// === Mixed types ===

int test_basic_mixed_types() {
    Person p = Person(25, 175.5, true);
    return p.age + int(p.height / 10.0) + (p.isStudent ? 1 : 0);
}

// int(17.55) = 17 -> 25 + 17 + 1 = 43
// @unimplemented(jit.q32)
// run: test_basic_mixed_types() == 43

float test_basic_assign_then_read_chained() {
    Point a = Point(0.0, 0.0);
    Point b = Point(9.0, 8.0);
    a = b;
    return a.x + a.y;
}

// @unimplemented(jit.q32)
// run: test_basic_assign_then_read_chained() ~= 17.0

bool test_basic_write_bool_member() {
    Person p = Person(10, 1.0, false);
    p.isStudent = true;
    return p.isStudent;
}

// @unimplemented(jit.q32)
// run: test_basic_write_bool_member() == true
